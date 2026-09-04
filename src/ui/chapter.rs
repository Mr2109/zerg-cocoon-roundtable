//! ui/chapter.rs — Chapter 视图（T7-3——2026-09-03）
//! 左：章节树（卷分组）——中：大纲+正文编辑（TextEdit multiline）+ 生成正文
//! 右：评审发现——顶：返回/生成章节大纲/批量生成
//! 后台任务：generate_chapters/generate_chapter_content+记忆+向量+评审（rt.spawn——busy 轮询）

use eframe::egui;
use egui::{Color32, RichText};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::db::models::Chapter;
use crate::ui::{RoundtableApp, View};

const DEFAULT_CHAPTERS: i64 = 10;
const VOLUME_SIZE: i64 = 5;

/// 后台生成任务：章节大纲（AI）
pub fn spawn_generate_outline(app: &mut RoundtableApp, sid: &str) {
    if app.gen_busy {
        return;
    }
    app.gen_busy = true;
    let db = app.db.clone();
    let sid_owned = sid.to_string();
    let done = Arc::new(AtomicBool::new(false));
    let done_task = done.clone();
    app.rt.spawn(async move {
        let ai = make_ai(&db);
        let _ = crate::engine::chapters::generate_chapters(
            &db,
            &ai,
            &sid_owned,
            DEFAULT_CHAPTERS,
            VOLUME_SIZE,
        )
        .await;
        done_task.store(true, Ordering::Relaxed);
    });
    app._outline_done = Some(done);
}

/// 后台生成正文：正文 → 角色状态 → 向量 → 评审（Web generate_chapter_content 全链）
pub fn spawn_generate_content(
    app: &mut RoundtableApp,
    sid: &str,
    volume: i64,
    chapter_number: i64,
) {
    if app.gen_busy {
        return;
    }
    app.gen_busy = true;
    let db = app.db.clone();
    let sid_owned = sid.to_string();
    let done = Arc::new(AtomicBool::new(false));
    let done_task = done.clone();
    app.rt.spawn(async move {
        let ai = make_ai(&db);
        let r = crate::engine::chapters::generate_chapter_content(
            &db,
            &ai,
            &sid_owned,
            volume,
            chapter_number,
        )
        .await;
        if let Ok(cid) = r {
            // 角色状态 + 向量 + 评审（Web 870 自动链）
            if let Ok(text) = db.get_chapter_content_text(cid).await {
                let _ = crate::engine::memory::update_character_states(
                    &db,
                    &sid_owned,
                    &text,
                    chapter_number,
                )
                .await;
                let _ = crate::engine::memory::embed_chapter(&db, &sid_owned, cid, &text).await;
            }
            let _ = crate::engine::review::review_chapter_content(&db, &ai, &sid_owned, cid, true)
                .await;
        }
        done_task.store(true, Ordering::Relaxed);
    });
    app._content_done = Some(done);
}

/// AI 构造（ai_connected=false 无 token → mock 空——返回不真跑——顶栏 ai_warn 已明示）
fn make_ai(_db: &crate::db::pool::Db) -> Box<crate::engine::discussion::BoxAi> {
    if crate::ui::ai_connected() {
        Box::new(Box::new(crate::ai::zerg::ZergProvider::from_env()))
    } else {
        Box::new(Box::new(crate::ai::mock::MockProvider::new(
            Vec::<&str>::new(),
        )))
    }
}

/// Chapter 视图
pub fn chapter_view(app: &mut RoundtableApp, ui: &mut egui::Ui, sid: &str) {
    let chapters = app
        .rt
        .block_on(app.db.get_chapters(sid))
        .ok()
        .unwrap_or_default();
    // 选中章数据
    let sel_ch = chapters
        .iter()
        .find(|c| c.id == app.sel_chapter_id)
        .cloned();
    // 正文编辑缓冲（按章 id 缓存——编辑后保存）
    let mut edit_buf: String = app
        .rt
        .block_on(app.db.get_chapter_content_text(app.sel_chapter_id))
        .ok()
        .unwrap_or_default();

    // 顶栏（返回会话在统一面包屑——此处只留生成控制——2026-09-04 三层收一层）
    egui::Panel::top("chap_top").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading("章节");
            ui.separator();
            // busy 复位检查
            if app.gen_busy {
                let outline_done = app
                    ._outline_done
                    .as_ref()
                    .map(|d| d.load(Ordering::Relaxed))
                    .unwrap_or(false);
                let content_done = app
                    ._content_done
                    .as_ref()
                    .map(|d| d.load(Ordering::Relaxed))
                    .unwrap_or(false);
                if outline_done || content_done {
                    app.gen_busy = false;
                    app._outline_done = None;
                    app._content_done = None;
                }
            }
            let btn = if app.gen_busy {
                "⏳ 生成中…"
            } else {
                "📋 生成章节大纲"
            };
            crate::ui::ai_warn(ui);
            if ui.button(btn).clicked() && !app.gen_busy {
                spawn_generate_outline(app, sid);
            }
            if chapters.is_empty() {
                ui.label(
                    RichText::new("（先生成大纲——AI 规划 10 章）")
                        .weak()
                        .small(),
                );
            }
        });
    });

    // 左：章节树
    egui::Panel::left("chap_tree")
        .default_size(240.0)
        .show(ui, |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("章节").strong());
            ui.separator();
            let mut vols: Vec<i64> = chapters.iter().map(|c| c.volume).collect();
            vols.sort();
            vols.dedup();
            if vols.is_empty() {
                ui.label(RichText::new("（空）").weak());
            }
            for vol in vols {
                ui.label(RichText::new(format!("第 {vol} 卷")).strong().small());
                for ch in chapters.iter().filter(|c| c.volume == vol) {
                    let has_content = ch.content.clone().unwrap_or_default().len() > 50;
                    let mark = if has_content { "✓" } else { "·" };
                    let label = format!(
                        "{mark} 第{}章 {}",
                        ch.chapter_number,
                        ch.title.clone().unwrap_or_default()
                    );
                    if ui
                        .selectable_label(app.sel_chapter_id == ch.id, label)
                        .clicked()
                    {
                        app.sel_chapter_id = ch.id;
                        edit_buf = app
                            .rt
                            .block_on(app.db.get_chapter_content_text(ch.id))
                            .ok()
                            .unwrap_or_default();
                    }
                    ui.add_space(1.0);
                }
            }
        });

    // 右：评审发现
    egui::Panel::right("chap_review")
        .default_size(260.0)
        .show(ui, |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("评审发现").strong());
            ui.separator();
            let reviews = app
                .rt
                .block_on(app.db.get_chapter_reviews(app.sel_chapter_id))
                .ok()
                .unwrap_or_default();
            if reviews.is_empty() {
                ui.label(
                    RichText::new("（生成正文后评审——发现自动显示）")
                        .weak()
                        .small(),
                );
            }
            for (pipeline, severity, issue, _loc) in &reviews {
                let color = match severity.as_str() {
                    "critical" => Color32::from_rgb(230, 110, 110),
                    "major" => Color32::from_rgb(230, 160, 90),
                    _ => Color32::from_rgb(200, 200, 130),
                };
                ui.label(
                    RichText::new(format!("[{pipeline}·{severity}]"))
                        .small()
                        .color(color),
                );
                let t: String = issue.chars().take(120).collect();
                ui.label(RichText::new(t).small());
                ui.add_space(3.0);
            }
        });

    // 中：正文编辑
    egui::CentralPanel::default().show(ui, |ui| {
        ui.add_space(4.0);
        match &sel_ch {
            None => {
                ui.label(RichText::new("← 选一章——或先生成大纲").weak());
            }
            Some(ch) => {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "第{}章 {}",
                            ch.chapter_number,
                            ch.title.clone().unwrap_or_default()
                        ))
                        .strong(),
                    );
                    ui.separator();
                    let has_content = ch.content.clone().unwrap_or_default().len() > 50;
                    let btn = if app.gen_busy {
                        "⏳ 生成中…"
                    } else if has_content {
                        "🔄 重新生成正文"
                    } else {
                        "✍️ 生成正文"
                    };
                    if ui.button(btn).clicked() && !app.gen_busy {
                        spawn_generate_content(app, sid, ch.volume, ch.chapter_number);
                    }
                    if ui.button("💾 保存编辑").clicked() {
                        let _ = app
                            .rt
                            .block_on(app.db.update_chapter_content(sid, ch.id, &edit_buf));
                    }
                });
                ui.separator();
                let outline = ch.outline.clone().unwrap_or_default();
                if !outline.is_empty() {
                    ui.collapsing("📋 大纲", |ui| {
                        ui.label(RichText::new(&outline).small());
                    });
                }
                ui.label(
                    RichText::new("正文（编辑后保存——生成会覆盖）")
                        .weak()
                        .small(),
                );
                egui::ScrollArea::vertical()
                    .max_height(ui.available_height() - 20.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut edit_buf)
                                .desired_rows(24)
                                .desired_width(f32::INFINITY)
                                .font(egui::TextStyle::Monospace),
                        );
                    });
            }
        }
    });
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(800));
}
