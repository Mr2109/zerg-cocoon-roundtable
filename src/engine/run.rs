//! engine/run.rs — 讨论主循环 run_discussion（T4-4——2026-09-03）
//! Web 版 engine.py run_discussion(1000-1124) 移植——断点续跑（M3）
//! 启动扫未完成会话 → current_block 续跑 → 逐块 process_block → 锁后进度落库
//! 块顺序由模板 blocks 决定（引擎通用——不知道小说）

use crate::ai::AiMessage;
use crate::db::pool::Db;
use crate::engine::discussion::{process_block, BoxAi, DiscussionState};
use crate::engine::nodes::RtFlowNode;
use crate::templates::{Block, ProjectTemplate};
use rusqlite::params;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

/// 运行摘要
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub sid: String,
    pub started_at: usize,
    pub blocks_done: usize,
    pub blocks_total: usize,
    pub completed: bool,
}

/// AI 停止守卫（M4-2 响应式停止——2026-09-03）：chat 前置 stop 检查——停止置位立即 Err
/// 引擎各处对 Err 的容忍（草案/讨论 if let Ok / map_err? 传播）会把"已停止"快速收敛到主循环 stop 分支
struct StopGuard<'a> {
    inner: &'a dyn crate::ai::AiProvider,
    stop: &'a AtomicBool,
}

#[async_trait::async_trait]
impl crate::ai::AiProvider for StopGuard<'_> {
    async fn chat(
        &self,
        msgs: &[AiMessage],
        max_tokens: i64,
    ) -> Result<crate::ai::AiReply, crate::ai::AiError> {
        if self.stop.load(Ordering::Relaxed) {
            return Err(crate::ai::AiError::Api("已停止（用户点停止）".into()));
        }
        self.inner.chat(msgs, max_tokens).await
    }
    fn name(&self) -> &str {
        self.inner.name()
    }
}

/// 逐块主循环（断点：current_block 起跑——quality_gate: 锁后评分 D 级重跑 ≤2——默认 false 对齐 Web 未接线）
pub async fn run_discussion(
    db: &Db,
    ai: &BoxAi,
    tmpl: &ProjectTemplate,
    sid: &str,
    stop_flag: &AtomicBool,
    quality_gate: bool,
) -> Result<RunSummary, String> {
    let session = db
        .get_session(sid)
        .await
        .map_err(|e| format!("会话不存在 {sid}: {e}"))?
        .ok_or_else(|| format!("会话不存在: {sid}"))?;

    let start_block = session.current_block as usize;
    let total = tmpl.blocks.len();

    // 重建锁定上下文（已锁块——fm 拼——Web load_locked_context）
    let mut state = DiscussionState::new(
        sid,
        &session.name,
        &session.novel_type,
        &session.length,
        &session.provider,
    )
    .with_input_vars(HashMap::from([
        ("input.topic".to_string(), session.name.clone()),
        ("input.length".to_string(), session.length.clone()),
        ("input.novel_type".to_string(), session.novel_type.clone()),
    ]));
    let mut ctx = String::new();
    let mut locked_names: Vec<String> = Vec::new();
    let all_templates = db
        .get_templates(sid, None)
        .await
        .map_err(|e| e.to_string())?;
    for blk in tmpl.blocks.iter() {
        let mut fields: HashMap<String, String> = HashMap::new();
        let mut is_locked = false;
        for t in &all_templates {
            if t.block_name.as_deref() == Some(&blk.name) {
                if t.locked == 1 {
                    is_locked = true;
                }
                if let (Some(fn_), Some(fv)) = (&t.field_name, &t.field_value) {
                    fields.insert(fn_.clone(), fv.clone());
                }
            }
        }
        if is_locked {
            locked_names.push(blk.name.clone());
            ctx.push_str(&format!(
                "\n【{}】{}\n",
                blk.name,
                fill_fm(&blk.fm, &blk.fields, &fields)
            ));
        }
    }
    state.locked_context = ctx;
    state.locked_blocks = locked_names.into_iter().collect();

    // 断点位置在已锁块之后？——current_block 已推进——从 current_block 起跑
    let blocks: Vec<&Block> = tmpl.blocks.iter().skip(start_block).collect();
    if blocks.is_empty() {
        db.update_session_progress(sid, total as i64, "completed")
            .await
            .map_err(|e| e.to_string())?;
        return Ok(RunSummary {
            sid: sid.into(),
            started_at: 0,
            blocks_done: total,
            blocks_total: total,
            completed: true,
        });
    }

    db.update_session_progress(sid, start_block as i64, "running")
        .await
        .map_err(|e| e.to_string())?;
    let started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as usize)
        .unwrap_or(0);

    // AI 包停止守卫——引擎每次 AI 调用前查 stop——响应式停止（不等当前 block 跑完）
    let guard = StopGuard {
        inner: ai.as_ref(),
        stop: stop_flag,
    };

    let mut done = start_block;
    // B2: DAG 推进——游标=块索引（novel 全隐式线性——next 空=顺序——与原 for 循环完全等价）
    // next 声明：当前块 next 非空→跳到 next[0] 对应索引（B1 gate 不通过后续语义 B3 next_by 扩展）
    let mut idx = start_block;
    let mut guard_counter = 0usize; // 环保底（next 声明错误时防死循环——loader 已拒环，双保险）
    while idx < total {
        let blk = &tmpl.blocks[idx];
        if stop_flag.load(Ordering::Relaxed) {
            // 停止——进度留在当前块——下次续跑
            db.update_session_progress(sid, idx as i64, "idle")
                .await
                .map_err(|e| e.to_string())?;
            return Ok(RunSummary {
                sid: sid.into(),
                started_at,
                blocks_done: done,
                blocks_total: total,
                completed: false,
            });
        }
        // 单块运行——按 kind 分发（B2：discussion 原状态机；single/gate 走 RtFlowNode）
        let mut retries = 0;
        let mut outcome = match blk.kind.as_str() {
            "single" | "gate" => {
                let mut vars: HashMap<String, String> = HashMap::new();
                for (nid, k, v) in db.get_flow_vars(sid).await.unwrap_or_default() {
                    vars.insert(format!("{nid}.{k}"), v);
                }
                for (k, v) in &state.input_vars {
                    vars.insert(k.clone(), v.clone());
                }
                let mut ctx = crate::engine::nodes::NodeCtx {
                    db,
                    ai: &guard,
                    state: &mut state,
                    vars,
                };
                // 直接按 kind 调对应节点（两个具体类型不可 disjoint match——分支内直接调）
                if blk.kind == "single" {
                    crate::engine::nodes::SingleNode
                        .execute(&mut ctx, blk)
                        .await
                } else {
                    crate::engine::nodes::GateNode.execute(&mut ctx, blk).await
                }
            }
            _ => process_block(db, &guard, tmpl, &mut state, blk)
                .await
                .map(|o| crate::engine::nodes::NodeOutcome {
                    locked: o.locked,
                    reason: o.reason,
                    produced: Vec::new(),
                }),
        };
        if quality_gate
            && outcome.as_ref().map(|o| o.locked).unwrap_or(false)
            && blk.kind == "discussion"
        {
            // 质量门禁仅对 discussion 节点（single/gate 无草案可评分——B1 定案）
            let mut worst: i64 = 0;
            while retries < 2 {
                worst = gate_score_block(db, &guard, tmpl, sid, idx, blk, &state).await?;
                if worst < 60 {
                    retries += 1;
                    db.add_message(
                        sid,
                        "系统",
                        &format!(
                            "⏳ {} 质量门禁未过（{worst} 分 D 级），重跑第 {retries} 次",
                            blk.name
                        ),
                        "system",
                        "{\"gate\":1}",
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                    // 清该块字段+消息重跑（防断点恢复误判——opinion 计数清零）
                    let bn2 = blk.name.clone();
                    let sid2 = sid.to_string();
                    db.query(move |c| {
                        c.execute(
                            "UPDATE templates SET field_value = '', locked = 0, score = 0 WHERE session_id = ?1 AND block_name = ?2",
                            params![sid2, bn2],
                        )?;
                        c.execute(
                            "DELETE FROM messages WHERE session_id = ?1 AND metadata LIKE ?2",
                            params![sid2, format!("%\"{}\"%", bn2)],
                        )?;
                        Ok(())
                    })
                    .await
                    .map_err(|e| e.to_string())?;
                    // 重跑（kind 分发——与首次执行同路）
                    outcome = match blk.kind.as_str() {
                        "single" | "gate" => unreachable!("质量门禁仅 discussion——上方已判"),
                        _ => process_block(db, &guard, tmpl, &mut state, blk)
                            .await
                            .map(|o| crate::engine::nodes::NodeOutcome {
                                locked: o.locked,
                                reason: o.reason,
                                produced: Vec::new(),
                            }),
                    };
                } else {
                    break;
                }
            }
            let _ = worst;
        }
        let ok = outcome.as_ref().map(|o| o.locked).unwrap_or(false);
        if let Err(e) = outcome {
            // 异常——进度留当前块——断点续跑
            let _ = db.update_session_progress(sid, idx as i64, "idle").await;
            return Err(e);
        }
        // B2 推进：gate 不通过→回跳 next_by 或声明 back_to；否则 next[0] 或顺序 +1
        let _ = ok;
        guard_counter += 1;
        if guard_counter > total * 3 {
            return Err("推进步数超限（next 声明疑似环）——终止".into());
        }
        let next_idx = if !blk.next.is_empty() {
            // 声明了 next——gate 不通过时原地重跑语义 B3 扩展；现在 next[0] 即跳
            tmpl.blocks
                .iter()
                .position(|b| blk.next[0] == format!("n{}", b.index) || &blk.next[0] == &b.name)
        } else {
            None
        };
        done = idx + 1; // blocks_done 语义保持「已处理到第几块」（线性兼容）
        db.update_session_progress(
            sid,
            done as i64,
            if done >= total {
                "completed"
            } else {
                "running"
            },
        )
        .await
        .map_err(|e| e.to_string())?;
        // 推进游标：显式 next 优先，否则顺序 +1
        idx = next_idx.unwrap_or(idx + 1);
    }
    let completed = done >= total;
    Ok(RunSummary {
        sid: sid.into(),
        started_at,
        blocks_done: done,
        blocks_total: total,
        completed,
    })
}

/// 块评分（写 DB score——返回字段最差分——Web 门禁 AVG(score>0) 同语义简化最差）
async fn gate_score_block(
    db: &Db,
    ai: &dyn crate::ai::AiProvider,
    tmpl: &ProjectTemplate,
    sid: &str,
    block_index: usize,
    blk: &Block,
    state: &DiscussionState,
) -> Result<i64, String> {
    // 收集该块锁定字段值
    let mut fv = std::collections::HashMap::new();
    let all = db
        .get_templates(sid, Some(block_index as i64))
        .await
        .map_err(|e| e.to_string())?;
    for t in &all {
        if let (Some(fn_), Some(fval)) = (&t.field_name, &t.field_value) {
            if !fval.is_empty() {
                fv.insert(fn_.clone(), fval.clone());
            }
        }
    }
    if fv.is_empty() {
        return Ok(100); // 无字段可评——不拦
    }
    let qr =
        crate::engine::quality::quality_check(ai, &blk.name, &fv, &state.locked_context).await?;
    let detail = serde_json::json!({ "scores": qr.scores, "grade": qr.grade }).to_string();
    // 全字段写同分（简化——Web 按字段——门禁用 AVG——最差保守）
    for (f, _) in &fv {
        db.update_template_score(sid, block_index as i64, f, qr.total, &detail)
            .await
            .map_err(|e| e.to_string())?;
    }
    let _ = tmpl;
    Ok(qr.total)
}

/// fm 模板填充（每个 {} 一个字段值——与 discussion 内一致——提取公共）
pub fn fill_fm(fm: &str, fs: &[String], vv: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = fm;
    for f in fs {
        if let Some(pos) = rest.find("{}") {
            out.push_str(&rest[..pos]);
            out.push_str(&crate::engine::utils::format_field_value(
                vv.get(f).cloned().unwrap_or_default().as_str(),
            ));
            rest = &rest[pos + 2..];
        } else {
            break;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::mock::MockProvider;
    use crate::db::pool::Db;
    use crate::templates::novel;
    use std::sync::atomic::AtomicBool;

    /// 完整主循环 mock：2 块跑完——completed
    #[tokio::test]
    async fn run_two_blocks_completes() {
        let _ = std::fs::remove_file("/tmp/yz_run1.db");
        let db = Db::open("/tmp/yz_run1.db").await.unwrap();
        db.create_session("r1", "完整跑", "玄幻", "长篇", "zerg", "novel")
            .await
            .unwrap();
        let tmpl = novel::novel_template();

        // mock 脚本：每块 草案→引导→5表态→总结（8 响应/块——2 块 16 响应）
        let mut script: Vec<String> = Vec::new();
        for bi in 0..2 {
            script.push(format!("草案1：\n故事核:第{bi}号世界设定——少年穿越修行大陆。\n\n草案2：\n故事核:都市异能者守护城市。\n\n草案3：\n故事核:星际冒险者探索未知。"));
            script.push("请作者表态。".to_string());
            for _ in 0..5 {
                script.push("草案1：满意。".to_string());
            }
            script.push("选中草案1。".to_string());
        }
        let refs: Vec<&str> = script.iter().map(|s| s.as_str()).collect();
        let ai: BoxAi = Box::new(MockProvider::new(refs));
        let stop = AtomicBool::new(false);
        let sum = run_discussion(&db, &ai, &tmpl, "r1", &stop, false)
            .await
            .unwrap();
        assert!(sum.completed, "全块跑完");
        assert_eq!(sum.blocks_done, 13, "模板 13 块全跑");
        assert_eq!(sum.blocks_total, 13);
        // 会话状态 completed
        let s = db.get_session("r1").await.unwrap().unwrap();
        assert_eq!(s.status, "completed");
        assert_eq!(s.current_block, 13);
        // 锁定上下文重建（跑后续块时含前块）——全部块锁定
        let locked = db.get_templates("r1", None).await.unwrap();
        let locked_cnt = locked.iter().filter(|t| t.locked == 1).count();
        assert!(locked_cnt >= 2, "块字段锁定——实际 {locked_cnt}");
        std::fs::remove_file("/tmp/yz_run1.db").ok();
    }

    /// 断点续跑：current_block=1（第一块完成）——续跑从第 2 块
    #[tokio::test]
    async fn resume_from_block() {
        let _ = std::fs::remove_file("/tmp/yz_run2.db");
        let db = Db::open("/tmp/yz_run2.db").await.unwrap();
        db.create_session("r2", "断点续跑", "都市", "中篇", "zerg", "novel")
            .await
            .unwrap();
        // 预置：index0（类型/篇幅）已锁——current_block=1（完成第 1 块）
        let tmpl = novel::novel_template();
        db.upsert_template("r2", 0, "类型", "玄幻", true, tmpl.blocks[0].name.as_str())
            .await
            .unwrap();
        db.upsert_template("r2", 0, "篇幅", "中篇", true, tmpl.blocks[0].name.as_str())
            .await
            .unwrap();
        db.update_session_progress("r2", 1, "idle").await.unwrap();

        // 从 index1（故事核）起跑——只准备故事核脚本（其余块弹尾自动锁）
        let mut script: Vec<String> = Vec::new();
        script.push("草案1：\n故事核:第二块设定——主角设定细化。\n\n草案2：\n故事核:备选设定二。\n\n草案3：\n故事核:备选设定三。".to_string());
        script.push("请作者表态。".to_string());
        for _ in 0..5 {
            script.push("草案1：满意。".to_string());
        }
        script.push("选中草案1。".to_string());
        let refs: Vec<&str> = script.iter().map(|s| s.as_str()).collect();
        let ai: BoxAi = Box::new(MockProvider::new(refs));
        let stop = AtomicBool::new(false);
        let sum = run_discussion(&db, &ai, &tmpl, "r2", &stop, false)
            .await
            .unwrap();
        assert!(sum.completed);
        // 从 current_block=1 续跑——index1 起 12 块跑完——current_block=13
        let s = db.get_session("r2").await.unwrap().unwrap();
        assert_eq!(s.current_block, 13);
        assert_eq!(s.status, "completed");
        // 预置 index0 值未被覆盖（续跑从 index1 起——index0 不重跑）
        let t = db.get_templates("r2", Some(0)).await.unwrap();
        assert!(
            t.iter().any(|x| x.field_value.as_deref() == Some("玄幻")),
            "预置类型保留"
        );
        std::fs::remove_file("/tmp/yz_run2.db").ok();
    }

    /// 质量门禁：评分 55（D 级）→ 重跑 → 评分 82 通过
    #[tokio::test]
    async fn gate_reruns_d_grade() {
        let _ = std::fs::remove_file("/tmp/yz_run3.db");
        let db = Db::open("/tmp/yz_run3.db").await.unwrap();
        // 预置 index0 完成（跳过类型块——减少脚本轮数）——但门禁只测一块完整跑：
        // 直接从 index0 起跑一块（故事核 index1 前是类型块 index0——类型块字段少）
        db.create_session("r3", "门禁", "玄幻", "长篇", "zerg", "novel")
            .await
            .unwrap();
        let tmpl = novel::novel_template();
        // 只跑第一块（index0 类型块）——跑完即完成（预置 current_block=1 让它只跑 index1?）
        // 简化：预置 index0 完成——只跑 index1（故事核）——但 13 块会全跑……
        // 用 stop_flag 控制？——最简单：模板只跑一块的断言放第一块后查 DB
        let mut script: Vec<String> = Vec::new();
        // index0（类型块）草案流（会被跑——因为 current_block=0）
        script.push("草案1：\n类型:玄幻\n篇幅:长篇\n\n草案2：\n类型:科幻\n篇幅:长篇\n\n草案3：\n类型:都市\n篇幅:长篇".to_string());
        script.push("请作者表态。".to_string());
        for _ in 0..5 {
            script.push("草案1：满意。".to_string());
        }
        script.push("选中草案1。".to_string());
        script.push(r#"{"scores":{"一致性":5,"完整性":4,"创意性":3,"可实现性":4},"total":55,"grade":"D","strengths":[],"weaknesses":["冲突不足"],"improvement_suggestions":"加强冲突"}"#.to_string());
        // 重跑 index0
        script.push("草案1：\n类型:玄幻\n篇幅:长篇\n\n草案2：\n类型:科幻\n篇幅:长篇\n\n草案3：\n类型:都市\n篇幅:长篇".to_string());
        script.push("请作者表态。".to_string());
        for _ in 0..5 {
            script.push("草案1：满意。".to_string());
        }
        script.push("选中草案1。".to_string());
        script.push(r#"{"scores":{"一致性":5,"完整性":4,"创意性":4,"可实现性":4},"total":86,"grade":"B","strengths":["冲突明确"],"weaknesses":[],"improvement_suggestions":""}"#.to_string());
        let refs: Vec<&str> = script.iter().map(|s| s.as_str()).collect();
        let ai: BoxAi = Box::new(MockProvider::new(refs));
        let stop = AtomicBool::new(false);
        let sum = run_discussion(&db, &ai, &tmpl, "r3", &stop, true)
            .await
            .unwrap();
        assert!(sum.completed, "全跑完（后续块弹尾自动锁）");
        let msgs = db.get_all_messages("r3").await.unwrap();
        let gate_msg = msgs
            .iter()
            .find(|m| m.content.as_deref().unwrap_or("").contains("质量门禁未过"));
        assert!(gate_msg.is_some(), "门禁消息存在");
        // 重跑后该块字段有分（86——重跑后的模板 score）
        let t = db.get_templates("r3", Some(0)).await.unwrap();
        assert!(
            t.iter().any(|x| x.score >= 80),
            "重跑后评分通过——实际分数: {:?}",
            t.iter().map(|x| x.score).collect::<Vec<_>>()
        );
        std::fs::remove_file("/tmp/yz_run3.db").ok();
    }
}
