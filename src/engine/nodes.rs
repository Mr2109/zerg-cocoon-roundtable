//! engine/nodes.rs — 四类流节点（B1——v1.0.1 节点层）
//! RtFlowNode trait：节点自描述（kind/version/retry——对齐 graphon Node 基类思想）
//! B1 落地：single/gate 最小实现；discussion 复用 process_block 状态机（run.rs 主循环直接分发——
//!   不强行过 trait——等价性保护优先，B2 主循环改造时统一走 RtFlowNode）
//! 铁律：novel 流行为不变（等价性专测守护）

use crate::ai::AiMessage;
use crate::db::pool::Db;
use crate::engine::discussion::DiscussionState;
use crate::templates::Block;
use std::collections::HashMap;

/// 节点执行上下文（变量池快照 + AI + 会话状态）
pub struct NodeCtx<'a> {
    pub db: &'a Db,
    pub ai: &'a dyn crate::ai::AiProvider,
    pub state: &'a mut DiscussionState,
    /// 变量表（node.field → value——A4 substitute_vars 数据源）
    pub vars: HashMap<String, String>,
}

/// 节点产出
pub struct NodeOutcome {
    pub locked: bool,
    pub reason: String,
    /// 产出写入变量池的 selector→value（gate 也写判定结果）
    pub produced: Vec<(String, String)>,
}

/// 重试策略（对接 RtError——Env 可重试故障；声明在节点上——graphon retry_config 思想）
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff_secs: u64,
}
impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            max_attempts: 3,
            backoff_secs: 2,
        }
    }
}

/// 流节点 trait（自描述——B2 主循环统一分发用）
pub trait RtFlowNode {
    fn kind(&self) -> &'static str;
    fn version(&self) -> u32 {
        1
    }
    fn retry_policy(&self) -> RetryPolicy {
        RetryPolicy::default()
    }
    fn execute<'a>(
        &'a self,
        ctx: &'a mut NodeCtx<'a>,
        block: &'a Block,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<NodeOutcome, String>> + Send + 'a>>;
}

// ── single 节点（单问 LLM——一次调用→变量池）──

pub struct SingleNode;

impl RtFlowNode for SingleNode {
    fn kind(&self) -> &'static str {
        "single"
    }
    fn execute<'a>(
        &'a self,
        ctx: &'a mut NodeCtx<'a>,
        block: &'a Block,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<NodeOutcome, String>> + Send + 'a>>
    {
        Box::pin(async move {
            let prompt = crate::engine::discussion::substitute_vars(&block.desc, &ctx.vars);
            let sys = format!(
                "你是任务执行助手。严格按输出格式要求完成，不要解释。任务：\n{}\n输出格式：每行 `字段名: 内容`。",
                prompt
            );
            let reply = ctx
                .ai
                .chat(
                    &[
                        AiMessage {
                            role: "system".into(),
                            content: sys,
                        },
                        AiMessage {
                            role: "user".into(),
                            content: prompt.clone(),
                        },
                    ],
                    0,
                )
                .await
                .map_err(|e| e.to_string())?;
            let node_id = format!("n{}", block.index);
            let mut produced = Vec::new();
            if block.fields.len() == 1 {
                // 单字段=全文
                let f = &block.fields[0];
                ctx.db
                    .set_flow_var(&ctx.state.sid, &node_id, f, &reply.content)
                    .await
                    .map_err(|e| e.to_string())?;
                produced.push((format!("{node_id}.{f}"), reply.content.clone()));
            } else {
                // 多字段按行 `字段名: 内容` 解析
                for line in reply.content.lines() {
                    if let Some((k, v)) = line.split_once(':') {
                        let k = k.trim();
                        if block.fields.iter().any(|f| f == k) {
                            ctx.db
                                .set_flow_var(&ctx.state.sid, &node_id, k, v.trim())
                                .await
                                .map_err(|e| e.to_string())?;
                            produced.push((format!("{node_id}.{k}"), v.trim().to_string()));
                        }
                    }
                }
            }
            Ok(NodeOutcome {
                locked: true,
                reason: "single 完成".into(),
                produced,
            })
        })
    }
}

// ── gate 节点（判定/分支——B1 数值+包含判定；human_confirm B3）──

pub struct GateNode;

impl RtFlowNode for GateNode {
    fn kind(&self) -> &'static str {
        "gate"
    }
    fn retry_policy(&self) -> RetryPolicy {
        RetryPolicy {
            max_attempts: 1,
            backoff_secs: 0,
        } // gate 判定即出——不重试
    }
    fn execute<'a>(
        &'a self,
        ctx: &'a mut NodeCtx<'a>,
        block: &'a Block,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<NodeOutcome, String>> + Send + 'a>>
    {
        Box::pin(async move {
            // B3: human_gate=every_step/key_points 且无人工裁决记录 → 暂停挂起（awaiting_human）
            // 暂停=落库进度+状态 awaiting_human——UI 确认卡（B3 UI）答复后置回 idle 续跑
            if block.human_gate != "none" {
                let decided = ctx
                    .db
                    .get_flow_var(&ctx.state.sid, &format!("n{}", block.index), "人工裁决")
                    .await
                    .map_err(|e| e.to_string())?;
                if decided.is_none() {
                    // 无裁决——挂起：会话状态 awaiting_human（断点续跑机制天然接管——重启后仍挂起直到人答复）
                    ctx.db
                        .update_session_progress(
                            &ctx.state.sid,
                            block.index as i64,
                            "awaiting_human",
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    // 引导消息（讨论流可见——人知道要确认什么）
                    let ask = crate::engine::discussion::substitute_vars(&block.desc, &ctx.vars);
                    crate::engine::discussion::log_confirm_request(
                        ctx.db,
                        &ctx.state.sid,
                        block,
                        &ask,
                    )
                    .await?;
                    return Ok(NodeOutcome {
                        locked: false,
                        reason: "awaiting_human（等待人工确认——UI 确认卡答复后继续）".into(),
                        produced: Vec::new(),
                    });
                }
                // 有裁决——按裁决走（通过/驳回）
                let verdict = decided.unwrap();
                let pass = verdict == "通过";
                log::info!("gate 节点 {}: 人工裁决={verdict}", block.name);
                return Ok(NodeOutcome {
                    locked: pass,
                    reason: format!("人工裁决: {verdict}"),
                    produced: vec![(format!("n{}.判定", block.index), verdict.clone())],
                });
            }
            // 自动判定路径（B1 原逻辑——human_gate=none）
            let cond = crate::engine::discussion::substitute_vars(&block.desc, &ctx.vars);
            let (pass, detail) = if let Some((lhs, kw)) = cond.split_once(" 包含 ") {
                let kw = kw.trim();
                let hit = lhs.contains(kw);
                (
                    hit,
                    format!("包含判定 '{kw}': {}", if hit { "命中" } else { "未命中" }),
                )
            } else if let Some(stripped) = cond.strip_suffix(" 非空") {
                let v = stripped.trim();
                (
                    !v.is_empty() && v != "_",
                    format!("非空判定: {} 字", v.chars().count()),
                )
            } else {
                (true, "无条件通过".into())
            };
            let node_id = format!("n{}", block.index);
            let verdict = if pass { "通过" } else { "不通过" };
            ctx.db
                .set_flow_var(&ctx.state.sid, &node_id, "判定", verdict)
                .await
                .map_err(|e| e.to_string())?;
            log::info!("gate 节点 {}: {verdict} ({detail})", block.name);
            Ok(NodeOutcome {
                locked: pass,
                reason: detail,
                produced: vec![(format!("{node_id}.判定"), verdict.into())],
            })
        })
    }
}

// ── tool 节点（B3——HTTP GET/本地脚本最小集——产出写变量池）──

pub struct ToolNode;

impl RtFlowNode for ToolNode {
    fn kind(&self) -> &'static str {
        "tool"
    }
    fn execute<'a>(
        &'a self,
        ctx: &'a mut NodeCtx<'a>,
        block: &'a Block,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<NodeOutcome, String>> + Send + 'a>>
    {
        Box::pin(async move {
            // desc 语法：`http <变量替换后的完整URL>` 或 `sh <变量替换后的脚本>`（首行决定形态）
            let spec = crate::engine::discussion::substitute_vars(&block.desc, &ctx.vars);
            let node_id = format!("n{}", block.index);
            let output = if let Some(url) = spec.strip_prefix("http ") {
                // HTTP GET（reqwest——超时 30s——限 64KB——no_proxy 直连对齐 zerg.rs 踩坑）
                http_get_text(url.trim())
                    .await
                    .map_err(|e| format!("tool http 失败: {e}"))?
            } else if let Some(script) = spec.strip_prefix("sh ") {
                // 本地脚本（bash -c——30s 超时——spawn_blocking 防 async 阻塞——stdout 截 64KB）
                let script = script.trim().to_string();
                let out = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    tokio::task::spawn_blocking(move || {
                        std::process::Command::new("bash")
                            .arg("-c")
                            .arg(&script)
                            .output()
                    }),
                )
                .await
                .map_err(|_| "tool sh 超时 30s".to_string())?
                .map_err(|e| format!("tool sh 启动失败: {e}"))?
                .map_err(|e| format!("tool sh 执行失败: {e}"))?
                .pipe_result()
                .map_err(|e| format!("tool sh 失败: {e}"))?;
                out
            } else {
                return Err(format!(
                    "tool 节点 {} desc 语法非法——需以 'http ' 或 'sh ' 开头",
                    block.name
                ));
            };
            // 产出写变量池（单字段全文）
            let f = block
                .fields
                .first()
                .ok_or_else(|| format!("tool 节点 {} 需至少 1 个 field 承载输出", block.name))?;
            let truncated: String = output.chars().take(65536).collect();
            ctx.db
                .set_flow_var(&ctx.state.sid, &node_id, f, &truncated)
                .await
                .map_err(|e| e.to_string())?;
            log::info!(
                "tool 节点 {}: 完成——输出 {} 字符",
                block.name,
                truncated.chars().count()
            );
            Ok(NodeOutcome {
                locked: true,
                reason: "tool 完成".into(),
                produced: vec![(format!("{node_id}.{f}"), truncated)],
            })
        })
    }
}

/// HTTP GET 文本（限 64KB/30s——no_proxy 直连——对齐 ai/zerg.rs 踩坑）
async fn http_get_text(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    Ok(text.chars().take(65536).collect())
}

/// tokio Command output → stdout 字符串（stderr 失败时入错误）
trait PipeResult {
    fn pipe_result(self) -> Result<String, String>;
}
impl PipeResult for std::process::Output {
    fn pipe_result(self) -> Result<String, String> {
        if !self.status.success() {
            let err = String::from_utf8_lossy(&self.stderr);
            return Err(format!(
                "exit {:?}: {}",
                self.status.code(),
                err.chars().take(200).collect::<String>()
            ));
        }
        Ok(String::from_utf8_lossy(&self.stdout).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_contains_judgement() {
        // 模拟 desc="check:{{n1.结论}} 包含 违约" 替换后
        let cond = "结论:本合同设违约条款 包含 违约";
        let (hit, _) = cond
            .split_once(" 包含 ")
            .map(|(l, k)| (l.contains(k.trim()), ()))
            .unwrap();
        assert!(hit);
    }

    #[test]
    fn gate_nonempty_judgement() {
        let cond = "有内容 非空";
        assert!(cond.strip_suffix(" 非空").unwrap().trim() != "_");
    }
}
