//! ui/session.rs — Session 视图（T7-2——egui 0.36——三栏：Block 进度/讨论流/模板字段）
//! 左：13 Block 进度——中：讨论流（消息流着色——全自动进度）——右：模板字段
//! 引擎后台：开始→rt.spawn(run_discussion)——每帧 DB 轮询（切走不断——M2）

use eframe::egui;
use egui::{Color32, RichText};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::db::models::Session;
use crate::templates::novel;
use crate::ui::{RoundtableApp, View};

/// 会话运行状态（后台线程标志）
pub struct SessionRuntime {
    pub running: Arc<AtomicBool>,
    /// 引擎线程已退出（true——UI 惰性清理 runtime——停止/完成后可重开）
    pub done: Arc<AtomicBool>,
}

impl RoundtableApp {
    /// 开始讨论（后台 spawn——模型=会话 provider 字段——新建表单选择）
    pub fn start_discussion(&mut self, sid: &str) {
        if self.runtimes.contains_key(sid) {
            return; // 已在跑
        }
        // 会话模型名（选择器写入 DB——ormith 兜底）
        let prov = self
            .rt
            .block_on(self.db.get_session(sid))
            .ok()
            .flatten()
            .map(|s| s.provider.clone())
            .unwrap_or_else(|| "ornith-1.5-35b".into());
        let db = self.db.clone();
        let sid_owned = sid.to_string();
        // running=false 时引擎在跑；置 true = 请求停止
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        self.runtimes.insert(
            sid.to_string(),
            SessionRuntime { running: stop.clone(), done: done.clone() },
        );
        let rt2 = self.rt.handle().clone();
        rt2.spawn(async move {
            let ai = make_ai(&prov);
            match crate::engine::run::run_discussion(&db, &ai, &novel::novel_template(), &sid_owned, &stop, false).await {
                Ok(summary) => {
                    eprintln!("[zerg-ui] 会话 {} 讨论完成: {}/{} blocks", sid_owned, summary.blocks_done, summary.blocks_total);
                    // 完成/停止落库可见（讨论流系统消息——完成出声——2026-09-04）
                    if summary.completed {
                        let _ = db.add_message(&sid_owned, "系统", &format!("✅ 讨论完成：{}/{} Block 全部跑完——可前往 📖 章节查看正文", summary.blocks_done, summary.blocks_total), "system", "").await;
                    } else {
                        let _ = db.add_message(&sid_owned, "系统", &format!("⏸ 已停止（Block {}/{}——点 ▶ 开始可继续）", summary.blocks_done + 1, summary.blocks_total), "system", "").await;
                    }
                }
                Err(e) => {
                    // 失败落库可见（系统消息）+ 日志——防"没动静"无解释
                    let msg = format!("❌ 引擎中断: {}", e);
                    eprintln!("[zerg-ui] 会话 {} 引擎错误: {}", sid_owned, msg);
                    let _ = db.add_message(&sid_owned, "系统", &msg, "system", "").await;
                }
            }
            done.store(true, Ordering::Relaxed); // 引擎退出——UI 下帧清理
        });
    }

    /// 停止讨论（终止指定会话后台——DB 状态留 idle 断点）
    pub fn stop_discussion(&self, sid: &str) {
        if let Some(rt) = self.runtimes.get(sid) {
            rt.running.store(true, Ordering::Relaxed);
        }
    }
}

/// AI 构造（ai_connected=false 无 token 时 mock 空脚本——UI 顶栏 ai_warn 已明示）
/// provider=会话模型名（新建表单选择——DB sessions.provider——默认 ornith-1.5-35b）
fn make_ai(prov: &str) -> Box<crate::engine::discussion::BoxAi> {
    if crate::ui::ai_connected() {
        let p = prov.trim();
        // 白名单校验（老会话 provider='zerg-ornith' 等旧值→回退 env 默认——防 404）
        if p.is_empty() || !crate::ui::MODELS.contains(&p) {
            eprintln!("[zerg-ui] provider '{}' 不在候选——回退 env 默认模型", p);
            Box::new(Box::new(crate::ai::zerg::ZergProvider::from_env()))
        } else {
            Box::new(Box::new(crate::ai::zerg::ZergProvider::with_model(p)))
        }
    } else {
        Box::new(Box::new(crate::ai::mock::MockProvider::new(Vec::<&str>::new())))
    }
}

/// Session 三栏视图
pub fn session_view(app: &mut RoundtableApp, ui: &mut egui::Ui, sid: &str) {
    let session: Option<Session> = app.rt.block_on(app.db.get_session(sid)).ok().flatten();
    let Some(s) = session else {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label("会话不存在");
        });
        return;
    };

    let tmpl = novel::novel_template();
    let templates = app.rt.block_on(app.db.get_templates(sid, None)).ok().unwrap_or_default();
    let messages = app.rt.block_on(app.db.get_all_messages(sid)).ok().unwrap_or_default();

    // 顶栏已上移统一面包屑（mod.rs rt_top——2026-09-04 三层收一层）——此处只画三栏

    // 左栏：Block 进度
    egui::Panel::left("blocks")
        .default_size(190.0)
        .show(ui, |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("Block 进度").strong());
            ui.separator();
            for (i, blk) in tmpl.blocks.iter().enumerate() {
                let locked = templates.iter().any(|t| t.locked == 1 && t.block_name.as_deref() == Some(&blk.name));
                let cur = s.current_block as usize == i;
                let (color, mark) = if locked {
                    (Color32::from_rgb(110, 200, 120), "✅")
                } else if cur {
                    (Color32::from_rgb(220, 180, 90), "▶")
                } else {
                    (Color32::GRAY, "·")
                };
                ui.label(RichText::new(format!("{mark} {}.{}", i + 1, blk.name)).color(color));
                ui.add_space(1.0);
            }
        });

    // 右栏：模板字段
    egui::Panel::right("fields")
        .default_size(300.0)
        .show(ui, |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("模板字段").strong());
            ui.separator();
            if templates.is_empty() {
                ui.label(RichText::new("（讨论开始后显示）").weak());
            }
            let mut shown = std::collections::HashSet::new();
            for t in &templates {
                let key = format!("{}_{}", t.block_index.unwrap_or(0), t.field_name.clone().unwrap_or_default());
                if shown.contains(&key) {
                    continue;
                }
                shown.insert(key);
                let bn = t.block_name.clone().unwrap_or_default();
                let fn_ = t.field_name.clone().unwrap_or_default();
                let locked = t.locked == 1;
                let val = t.field_value.clone().unwrap_or_default();
                ui.label(RichText::new(format!("{bn}·{fn_}")).small().strong());
                let text: String = val.chars().take(160).collect();
                ui.label(RichText::new(if text.is_empty() { "（空）" } else { &text }).small());
                if locked {
                    ui.label(RichText::new("🔒 已锁定").small().color(Color32::from_rgb(110, 200, 120)));
                }
                ui.add_space(3.0);
            }
        });

    // 中央：讨论流
    egui::CentralPanel::default().show(ui, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("讨论流").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // 字体放大/缩小（2 号需求——2026-09-03）
                if ui.button("A+").clicked() {
                    app.msg_px = (app.msg_px + 1.0).min(24.0);
                }
                ui.label(RichText::new(format!("{}px", app.msg_px as i32)).weak().small());
                if ui.button("A−").clicked() {
                    app.msg_px = (app.msg_px - 1.0).max(9.0);
                }
            });
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
            let mut last_sender = String::new();
            for m in &messages {
                let sender = m.sender.clone();
                let sender_type = m.sender_type.clone();
                let content = m.content.clone().unwrap_or_default();
                let color = match sender_type.as_str() {
                    "opinion" => Color32::from_rgb(140, 180, 240),
                    "plan" => Color32::from_rgb(240, 200, 130),
                    "guide" => Color32::from_rgb(170, 220, 170),
                    "discussion" => Color32::from_rgb(200, 200, 230),
                    "system" => Color32::GRAY,
                    _ => Color32::from_rgb(220, 220, 220),
                };
                if sender != last_sender {
                    ui.add_space(4.0);
                    last_sender = sender.clone();
                }
                ui.label(RichText::new(format!("【{sender}】")).color(color).strong().size(app.msg_px * 0.9));
                let text: String = content.chars().take(2000).collect();
                for para in text.split('\n').filter(|p| !p.is_empty()) {
                    ui.label(RichText::new(para).size(app.msg_px));
                }
                ui.add_space(2.0);
            }
        });
    });
    // 轮询刷新（0.5s——后台写入可见）
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
}
