//! ui/builder.rs — AI 建流回路（C4d——v1.0.1 工坊核心闭环）
//! 流程（对齐设计 C-4/C4d + Dify workflow-generator 范式）：
//!   ① 需求 → AI 规划（单轮——出结构化方案说明）
//!   ② AI 生成 flow.json（validate-loop：生成→装载器校验→错误回喂修复——≤10 次）
//!   ③ 通过→出 diff 提议卡片（collab 面板）——人采纳/拒绝——同一装载器无特权通道
//! 重复指纹熔断（连续 3 次错误清单不变→停——防 AI 死磕同错）

use crate::ai::{AiMessage, AiProvider};
use eframe::egui;
use egui::{Color32, RichText};

/// 建流回路状态
pub struct Builder {
    /// 用户需求（输入框缓冲）
    pub requirement: String,
    /// 回路进行中标志
    pub running: bool,
    /// 当前阶段提示（Planning…/Generating 3/10…/✅ 完成）
    pub phase: String,
    /// 结果接收（后台线程→UI 帧）
    rx: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Builder {
            requirement: String::new(),
            running: false,
            phase: String::new(),
            rx: None,
        }
    }
}

const MAX_VALIDATE_ATTEMPTS: usize = 10; // 对齐 n8n（Mr2109授权调研定案）
const MAX_SAME_ERRORS: usize = 3; // 重复指纹熔断

/// 建流系统提示（JSON 生成——schema 内嵌——few-shot 精简）
const GEN_SYSTEM: &str = r#"你是任务流架构师。根据需求生成一份任务流声明 JSON（flow.json）。

必须遵守的 schema：
{
  "id": "小写字母数字下划线",
  "name": "中文名",
  "version": 1,
  "inputs": [{"key": "xxx", "label": "中文名", "required": true, "options": [], "default": null}],
  "roles": {
    "moderator": {"name": "主持人", "role": "……"},
    "panel": [ {"name": "角色名", "zi": "字XX", "specialty": "专长", "description": "职责"} ]  // 1-8 个角色
  },
  "nodes": [  // 按执行顺序
    {"id": "n0", "kind": "discussion|single|tool|gate", "name": "节点名",
     "desc": "引导文案，可引用 {{nX.字段}} 或 {{input.key}}（必须已声明）",
     "fields": ["产出字段"], "fm": "= 字段:{}", "next": [], "human_gate": "none|key_points|every_step", "model": ""}
  ],
  "gate": {"pass_score": 80, "max_cycle": 3}
}

规则：
1. discussion=多角色讨论节点（fields 必填）；single=单次 LLM（fields 承载产出）；tool=HTTP/脚本（desc 以 "http " 或 "sh " 开头）；gate=判定（desc 可写 "<值> 包含 <kw>" / "<值> 非空"）或人工确认（human_gate 非 none）
2. 节点 id 必须 n0/n1/n2… 连续；next 留空=顺序执行
3. {{引用}} 只能指向已声明节点 fields 或 inputs key（input. 前缀）
4. 角色团按任务领域配置（合同审查用法务/财务等专长角色）——不要照抄小说角色
5. 只输出 JSON，不要 markdown 代码块标记，不要解释。"#;

/// 校验错误清单格式化为回喂文本
fn fmt_errors(errs: &[String]) -> String {
    let mut s = String::from("你生成的 JSON 有以下错误，请修正后重新输出完整 JSON：\n");
    for e in errs {
        s.push_str(&format!("- {e}\n"));
    }
    s.push_str("\n只输出修正后的完整 JSON。");
    s
}

/// 后台执行建流回路（规划→生成→validate-loop——返回最终 JSON 或错误摘要）
async fn build_flow(
    ai: &dyn AiProvider,
    requirement: &str,
    base_json: Option<String>,
) -> Result<String, String> {
    // ① 规划（单轮——方案说明；不阻塞主链路——失败仅日志）
    let plan_prompt = format!(
        "用户想用圆桌派（AI 群策引擎）完成以下任务：\n{requirement}\n\n请用 3-5 句话给出任务流设计方案：几个节点、各节点职责、角色团怎么配、哪里需要人工确认。不输出 JSON。"
    );
    let plan = ai
        .chat(
            &[AiMessage {
                role: "user".into(),
                content: plan_prompt,
            }],
            0,
        )
        .await
        .map(|r| r.content)
        .unwrap_or_else(|e| format!("（规划失败：{e}——继续生成）"));
    log::info!(
        "AI 建流规划: {}",
        plan.chars().take(200).collect::<String>()
    );

    // ② 生成 + validate-loop
    let gen_user = format!("{plan}\n\n用户需求原文：\n{requirement}\n\n请生成任务流 JSON。");
    let mut messages = vec![AiMessage {
        role: "system".into(),
        content: GEN_SYSTEM.to_string(),
    }];
    if let Some(base) = &base_json {
        // 有基线=修改现有流（把当前 JSON 给 AI 参考）
        messages.push(AiMessage {
            role: "user".into(),
            content: format!("当前任务流 JSON：\n{base}\n\n修改需求：{gen_user}"),
        });
    } else {
        messages.push(AiMessage {
            role: "user".into(),
            content: gen_user,
        });
    }

    let mut last_errs: Vec<String> = Vec::new();
    let mut same_count = 0usize;
    for attempt in 1..=MAX_VALIDATE_ATTEMPTS {
        let reply = ai
            .chat(&messages, 0)
            .await
            .map_err(|e| e.to_string())?
            .content;
        // 剥 markdown 代码块（AI 常见包装）
        let json_text = strip_code_fence(&reply);
        match crate::templates::loader::load_flow_str(&json_text) {
            Ok(t) => {
                log::info!("AI 建流: 第 {attempt} 次校验通过——{} 节点", t.blocks.len());
                // 重序列化（规范化字段——serde 保障）
                return serde_json::to_string_pretty(&serde_json::json!({
                    "id": t.project_type, "name": t.project_type, "version": 1,
                }))
                .map(|_| json_text)
                .map_err(|e| e.to_string());
            }
            Err(errs) => {
                log::warn!("AI 建流: 第 {attempt} 次校验失败（{} 条）", errs.len());
                // 重复指纹熔断
                if errs == last_errs {
                    same_count += 1;
                    if same_count >= MAX_SAME_ERRORS {
                        return Err(format!(
                            "连续 {MAX_SAME_ERRORS} 次相同错误——AI 疑似死磕，熔断。最后错误：{}",
                            errs.join("; ")
                        ));
                    }
                } else {
                    same_count = 0;
                    last_errs = errs.clone();
                }
                // 错误回喂
                messages.push(AiMessage {
                    role: "assistant".into(),
                    content: reply,
                });
                messages.push(AiMessage {
                    role: "user".into(),
                    content: fmt_errors(&errs),
                });
            }
        }
    }
    Err(format!(
        "{} 次校验未过——放弃。最后错误：{}",
        MAX_VALIDATE_ATTEMPTS,
        last_errs.join("; ")
    ))
}

/// 剥 markdown 代码块标记（```json ... ``` 或 ``` ... ```）
fn strip_code_fence(s: &str) -> String {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")) {
        if let Some(inner) = rest.strip_suffix("```") {
            return inner.trim().to_string();
        }
    }
    t.to_string()
}

/// UI：请 AI 改按钮 → 启动后台回路
pub fn start_build(app: &mut crate::ui::RoundtableApp) {
    if app.flow_builder.running {
        return;
    }
    if !crate::ui::ai_connected() {
        app.flow_builder.phase = "❌ 无 AI 连接（YZ_GATEWAY_TOKEN 未配置）".into();
        return;
    }
    let requirement = app.flow_builder.requirement.trim().to_string();
    if requirement.is_empty() {
        app.flow_builder.phase = "❌ 请先输入需求".into();
        return;
    }
    app.flow_builder.running = true;
    app.flow_builder.phase = "Planning…（AI 规划中）".into();

    // AI：zerg provider（会话模型默认 ornith——from_env）
    let ai: Box<dyn AiProvider> = Box::new(crate::ai::zerg::ZergProvider::from_env());
    let base = if app.workshop.draft.is_empty() {
        None
    } else {
        Some(app.workshop.draft.clone())
    };
    let req = requirement.clone();
    let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
    app.flow_builder.rx = Some(rx);

    app.rt.spawn(async move {
        let t0 = std::time::Instant::now();
        let result = build_flow(ai.as_ref(), &req, base).await;
        log::info!("AI 建流完成: {}ms", t0.elapsed().as_millis());
        let _ = tx.send(result);
    });
}

/// UI 帧轮询：结果→提议卡片（走 collab.propose——diff+预检+人裁决）
pub fn poll_build(app: &mut crate::ui::RoundtableApp) {
    if !app.flow_builder.running {
        return;
    }
    // try_recv 直接探测（空=还再跑——不消费）
    let probe = match app.flow_builder.rx.as_ref() {
        Some(rx) => match rx.try_recv() {
            Ok(msg) => Some(Ok::<Result<String, String>, std::sync::mpsc::RecvError>(
                msg,
            )),
            Err(std::sync::mpsc::TryRecvError::Empty) => return, // 还在跑
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // 通道关闭=线程异常退出没发结果——合成错误消息
                Some(Ok(Err("后台线程异常退出".to_string())))
            }
        },
        None => return,
    };
    app.flow_builder.rx = None; // 消费掉
    match probe {
        Some(Ok(Ok(new_json))) => {
            app.flow_builder.phase = "✅ 生成完成——出提议卡片待人裁决".into();
            app.collab.propose(
                format!(
                    "AI 建流：{}",
                    app.flow_builder
                        .requirement
                        .chars()
                        .take(40)
                        .collect::<String>()
                ),
                new_json,
                &app.workshop.draft,
            );
        }
        Some(Ok(Err(e))) => {
            app.flow_builder.phase = format!("❌ {e}");
        }
        _ => {
            app.flow_builder.phase = "❌ 后台线程异常".into();
        }
    }
    app.flow_builder.running = false;
}

/// UI：建流区渲染（工坊代码视图工具条下——需求输入+按钮+阶段提示）
pub fn builder_ui(app: &mut crate::ui::RoundtableApp, ui: &mut egui::Ui) {
    poll_build(app);
    ui.horizontal(|ui| {
        ui.label(RichText::new("🤖 请 AI 建/改流：").small());
        ui.add(
            egui::TextEdit::singleline(&mut app.flow_builder.requirement)
                .hint_text("例：建一个周报生成流——收集本周要点→讨论总结→输出报告")
                .desired_width(420.0),
        );
        let can = !app.flow_builder.running;
        if ui
            .add_enabled(
                can,
                egui::Button::new(if app.workshop.draft.is_empty() {
                    "🤖 AI 建流"
                } else {
                    "🤖 AI 修改当前流"
                })
                .small(),
            )
            .clicked()
        {
            start_build(app);
        }
        if app.flow_builder.running {
            ui.label(RichText::new("⏳").small());
        }
    });
    if !app.flow_builder.phase.is_empty() {
        let color = if app.flow_builder.phase.starts_with('✅') {
            Color32::from_rgb(110, 200, 120)
        } else if app.flow_builder.phase.starts_with('❌') {
            Color32::from_rgb(230, 110, 110)
        } else {
            Color32::from_rgb(220, 180, 90)
        };
        ui.label(RichText::new(&app.flow_builder.phase).small().color(color));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_code_fence_works() {
        assert_eq!(strip_code_fence("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_code_fence("{\"a\":1}"), "{\"a\":1}");
        assert_eq!(strip_code_fence("```\n{\"a\":1}\n```"), "{\"a\":1}");
    }

    #[test]
    fn fmt_errors_lists() {
        let s = fmt_errors(&["[schema] x".into(), "[graph] y".into()]);
        assert!(s.contains("[schema] x") && s.contains("[graph] y"));
        assert!(s.contains("重新输出完整 JSON"));
    }
}
