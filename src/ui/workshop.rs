//! ui/workshop.rs — 工坊（C4a/C4b——人机共用任务流工作台）
//! 布局：模板库(左) | 工作区(中——代码视图/试跑tab) | 校验状态条(底部)
//! 铁律：所有修改走装载器校验（唯一裁判）——校验过才可保存
//! C4b 试跑：MockProvider 会话——真跑节点顺序/变量传递/门禁分支——零 token

use eframe::egui;
use egui::{Color32, RichText};

use crate::templates::loader;

/// 试跑日志行（C4b——节点级进出记录）
#[derive(Debug, Clone)]
pub struct TraceRow {
    pub node: String,   // 节点名
    pub event: String,  // 进入/完成/跳过/挂起/变量写入
    pub detail: String, // 产出摘要/原因
}

/// 工坊状态（挂 RoundtableApp）
pub struct Workshop {
    /// 正在编辑的模板 id（None=未选中——显示模板库/新建）
    pub editing: Option<String>,
    /// JSON 编辑缓冲（草稿——未保存）
    pub draft: String,
    /// 上次校验结果（None=未校验；Some(Err)=错误清单；Some(Ok(_))=可保存）
    pub last_check: Option<Result<(), Vec<String>>>,
    /// 保存前基线（内容未变则保存按钮禁用）
    pub saved_baseline: Option<String>,
    /// 提示消息
    pub toast: Option<String>,
    /// 工作区 tab（0=代码 1=试跑）
    pub tab: usize,
    /// C4b: 试跑日志（节点级 trace）
    pub trace: Vec<TraceRow>,
    /// C4b: 试跑中标志（防重入）
    pub dry_running: bool,
    /// C4b: 试跑完成标志（false=还在跑）
    pub dry_done: bool,
    /// C4b: 试跑结果接收（后台线程→UI 帧）
    dry_rx: Option<std::sync::mpsc::Receiver<String>>,
    /// C4b: 试跑会话 id
    dry_sid: Option<String>,
    /// C4b: runtime handle（poll 清理用）
    dry_rt: Option<tokio::runtime::Handle>,
    /// C4c: 保存标志（save 成功置位——workshop_view 读后清+入协作流水）
    pub saved_flag: bool,
    /// C3: 画布选中节点 id（空=未选）
    pub canvas_sel: String,
    /// C3: 属性面板缓冲（sync_for=缓冲对应的节点 id）
    pub canvas_buf: CanvasBuf,
}

/// C3: 画布属性面板编辑缓冲
#[derive(Debug, Clone, Default)]
pub struct CanvasBuf {
    pub sync_for: String,
    pub name: String,
    pub kind: String,
    pub gate: bool,
    pub desc: String,
    pub fields: String,
    pub next: String,
}

impl Workshop {
    pub fn new() -> Self {
        Workshop {
            editing: None,
            draft: String::new(),
            last_check: None,
            saved_baseline: None,
            toast: None,
            tab: 0,
            trace: Vec::new(),
            dry_running: false,
            dry_done: false,
            dry_rx: None,
            dry_sid: None,
            dry_rt: None,
            saved_flag: false,
            canvas_sel: String::new(),
            canvas_buf: CanvasBuf::default(),
        }
    }

    /// 打开模板进编辑（读文件——内置 novel 无文件则导出）
    pub fn open(&mut self, id: &str, dir: &std::path::Path) {
        let path = dir.join(format!("{id}.flow.json"));
        let text = if path.exists() {
            std::fs::read_to_string(&path).unwrap_or_default()
        } else if id == "novel" {
            crate::templates::novel::export_novel_flow_json()
        } else {
            String::new()
        };
        self.draft = text.clone();
        self.saved_baseline = Some(text);
        self.editing = Some(id.to_string());
        self.trace.clear();
        self.revalidate();
    }

    /// 新建空白模板骨架
    pub fn new_flow(&mut self) {
        self.draft = r#"{
  "id": "new_flow",
  "name": "新任务流",
  "version": 1,
  "inputs": [],
  "roles": {
    "moderator": {"name": "主持人", "role": "引导"},
    "panel": [
      {"name": "角色一", "zi": "字一", "specialty": "专长", "description": "负责……"}
    ]
  },
  "nodes": [
    {"id": "n0", "kind": "discussion", "name": "议题一", "desc": "", "fields": ["结论"], "fm": "= 结论:{}"}
  ]
}
"#
        .to_string();
        self.saved_baseline = None;
        self.editing = Some("new_flow".to_string());
        self.trace.clear();
        self.revalidate();
    }

    /// 校验当前草稿（装载器即唯一裁判）
    pub fn revalidate(&mut self) {
        self.last_check = Some(match loader::load_flow_str(&self.draft) {
            Ok(_) => Ok(()),
            Err(errs) => Err(errs),
        });
    }

    /// 保存（校验过才落盘——文件名=模板 id）
    pub fn save(&mut self, dir: &std::path::Path) {
        let Some(id) = self.editing.clone() else {
            return;
        };
        // id 合法性（文件名安全——字母数字下划线）
        if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            self.toast = Some("❌ id 只能含字母/数字/下划线".into());
            return;
        }
        // 保存前强制重校验（防编辑后未触发防抖）
        self.revalidate();
        match &self.last_check {
            Some(Ok(())) => {
                let path = dir.join(format!("{id}.flow.json"));
                match std::fs::write(&path, &self.draft) {
                    Ok(()) => {
                        self.saved_baseline = Some(self.draft.clone());
                        self.toast = Some(format!("✅ 已保存 {path:?}"));
                        log::info!("工坊保存: {path:?}");
                        // C4c: 人改流水（由调用方协作面板入账——此处回传标记）
                        self.saved_flag = true;
                    }
                    Err(e) => self.toast = Some(format!("❌ 写文件失败: {e}")),
                }
            }
            Some(Err(errs)) => {
                self.toast = Some(format!("❌ 校验未过（{} 条错误）——修完再保存", errs.len()));
            }
            None => {}
        }
    }

    /// C4b: 启动试跑（校验过才跑——Mock AI——同步小流程直接 block_on）
    pub fn dry_run(&mut self, db: &crate::db::pool::Db, rt: &tokio::runtime::Handle) {
        if self.dry_running {
            return;
        }
        self.revalidate();
        if !matches!(self.last_check, Some(Ok(()))) {
            self.toast = Some("❌ 先修完校验错误再试跑".into());
            return;
        }
        let Ok(tmpl) = loader::load_flow_str(&self.draft) else {
            return;
        };
        self.dry_running = true;
        self.dry_done = false;
        self.trace.clear();
        self.tab = 1; // 自动切到试跑 tab 看过程

        let sid = format!("ws_dry_{}", std::process::id());
        let db2 = db.clone();
        let handle = rt.clone();
        // Mock 脚本：每节点按 kind 预置响应（discussion 8 响应/块——草案引导表态总结；single 1 响应按 fields 生成）
        let mut script: Vec<String> = Vec::new();
        for b in &tmpl.blocks {
            match b.kind.as_str() {
                "discussion" => {
                    let fv: Vec<String> = b
                        .fields
                        .iter()
                        .map(|f| format!("{f}: 试跑内容示例"))
                        .collect();
                    script.push(format!(
                        "草案1：\n{}\n\n草案2：\n{}\n\n草案3：\n{}",
                        fv.join("\n"),
                        fv.join("\n"),
                        fv.join("\n")
                    ));
                    script.push("请作者表态。".into());
                    for _ in 0..b.fields.len().max(1) {
                        script.push("草案1：满意。".into());
                    }
                    script.push("选中草案1。".into());
                }
                "single" | "tool" => {
                    let lines: Vec<String> =
                        b.fields.iter().map(|f| format!("{f}: 试跑产出")).collect();
                    script.push(lines.join("\n"));
                }
                _ => {} // gate 不调 AI
            }
        }
        let refs: Vec<&str> = script.iter().map(|s| s.as_str()).collect();
        let mock: Box<dyn crate::ai::AiProvider> =
            Box::new(crate::ai::mock::MockProvider::new(refs));

        // 试跑用真实会话行（project_type 用临时 id——不与正式会话混——库清理由 rerun 时做）
        let ptype = self.editing.clone().unwrap_or_default();
        let db3 = db2.clone();
        let sid_create = sid.clone();
        handle.block_on(async move {
            let _ = db3
                .create_session(&sid_create, "工坊试跑", "-", "-", "mock", &ptype)
                .await;
        });

        // 后台跑主循环（复用 run_discussion——Mock AI 零 token）
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let ws_flag = self.dry_done_swap_handle();
        let sid3 = sid.clone();
        let _ = sid3;
        let sid_run = sid.clone();
        rt.spawn(async move {
            let sum =
                crate::engine::run::run_discussion(&db2, &mock, &tmpl, &sid_run, &stop, false)
                    .await;
            let msg = match sum {
                Ok(s) => format!(
                    "{}——{}/{} 节点完成",
                    if s.completed {
                        "✅ 试跑完成"
                    } else {
                        "⏸ 中止"
                    },
                    s.blocks_done,
                    s.blocks_total
                ),
                Err(e) => format!("❌ 试跑失败: {e}"),
            };
            let _ = tx.send(msg);
            ws_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        // 收尾 channel 挂到 self（UI 帧轮询）
        self.dry_rx = Some(rx);
        self.dry_sid = Some(sid);
        self.dry_rt = Some(rt.clone());
    }

    /// 内部：dry_done 位（后台线程置——UI 帧 读）
    fn dry_done_swap_handle(&mut self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))
    }

    /// C4b: UI 帧轮询试跑结果（channel 收尾——变量池落 trace）
    pub fn poll_dry_run(&mut self, db: &crate::db::pool::Db) {
        if let Some(rx) = &self.dry_rx {
            if let Ok(msg) = rx.try_recv() {
                self.toast = Some(msg);
                self.dry_running = false;
                let sid = self.dry_sid.clone().unwrap_or_default();
                let rt = self.dry_rt.clone();
                // 试跑后拉变量池做 trace（节点级产出）
                if let Some(rt) = rt {
                    let db2 = db.clone();
                    let sid2 = sid.clone();
                    let rows =
                        rt.block_on(
                            async move { db2.get_flow_vars(&sid2).await.unwrap_or_default() },
                        );
                    self.trace = rows
                        .into_iter()
                        .map(|(nid, k, v)| TraceRow {
                            node: nid.clone(),
                            event: "产出".into(),
                            detail: format!("{k} = {}", v.chars().take(60).collect::<String>()),
                        })
                        .collect();
                    // 清试跑会话（不留垃圾）
                    let db3 = db.clone();
                    let sid3 = sid.clone();
                    rt.block_on(async move {
                        let _ = db3
                            .query(move |c| {
                                c.execute("DELETE FROM messages WHERE session_id=?1", [&sid3])?;
                                c.execute("DELETE FROM flow_vars WHERE sid=?1", [&sid3])?;
                                c.execute("DELETE FROM templates WHERE session_id=?1", [&sid3])?;
                                c.execute("DELETE FROM sessions WHERE id=?1", [&sid3])?;
                                Ok(())
                            })
                            .await;
                    });
                }
                self.dry_rx = None;
            }
        }
    }
}

/// 工坊主视图（三区——左模板库/中代码+试跑/右协作者/底校验条）
pub fn workshop_view(app: &mut crate::ui::RoundtableApp, ui: &mut egui::Ui) {
    // 轮询试跑结果
    app.workshop.poll_dry_run(&app.db);
    // C4c: 保存成功→人改流水入账
    if app.workshop.saved_flag {
        app.workshop.saved_flag = false;
        let what = app
            .workshop
            .editing
            .clone()
            .map(|id| format!("保存 {id}"))
            .unwrap_or_else(|| "保存".into());
        app.collab.log_human(what);
    }

    // 右：协作者面板（C4c——AI 提议卡片+变更流水）
    egui::Panel::right("ws_collab")
        .default_size(280.0)
        .resizable(true)
        .show_inside(ui, |ui| {
            crate::ui::collab::collab_panel(app, ui);
        });

    // 左：模板库
    egui::Panel::left("ws_lib")
        .default_size(200.0)
        .show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("模板库").strong());
            ui.separator();
            let mut open: Option<String> = None;
            for (id, name) in &app.available_templates {
                let editing_here = app.workshop.editing.as_deref() == Some(id.as_str());
                if ui
                    .selectable_label(editing_here, RichText::new(name).size(13.0))
                    .clicked()
                {
                    open = Some(id.clone());
                }
            }
            ui.separator();
            if ui.button("＋ 新建空白流").clicked() {
                app.workshop.new_flow();
            }
            if let Some(id) = open {
                app.workshop
                    .open(&id, &crate::templates::default_templates_dir());
            }
        });

    // 中：tab（代码/试跑）+ 底部校验条
    egui::CentralPanel::default().show_inside(ui, |ui| {
        if app.workshop.editing.is_none() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.heading("🛠 工坊");
                ui.weak("左侧选择模板开始编辑——或新建空白流");
                ui.weak("所有修改经装载器校验——人写与 AI 写走同一裁判");
            });
            return;
        }
        let id = app.workshop.editing.clone().unwrap_or_default();
        // 工具条
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("编辑: {id}")).strong());
            let dirty = app
                .workshop
                .saved_baseline
                .as_ref()
                .map(|b| *b != app.workshop.draft)
                .unwrap_or(true);
            if dirty {
                ui.label(
                    RichText::new("● 未保存")
                        .color(Color32::from_rgb(230, 160, 90))
                        .small(),
                );
            }
            if ui.add_enabled(dirty, egui::Button::new("💾 保存")).clicked() {
                app.workshop
                    .save(&crate::templates::default_templates_dir());
            }
            if ui.button("↺ 重载文件").clicked() {
                if let Some(id) = app.workshop.editing.clone() {
                    app.workshop
                        .open(&id, &crate::templates::default_templates_dir());
                }
            }
            // C4b: 试跑按钮（校验过才可用）
            let can_run = matches!(app.workshop.last_check, Some(Ok(())))
                && !app.workshop.dry_running;
            if ui
                .add_enabled(can_run, egui::Button::new("▶ 试跑(Mock)"))
                .clicked()
            {
                let db = app.db.clone();
                let rt = app.rt.handle().clone();
                app.workshop.dry_run(&db, &rt);
            }
            if app.workshop.dry_running {
                ui.label(RichText::new("⏳ 试跑中…").color(Color32::from_rgb(220, 180, 90)).small());
            }
            if let Some(t) = &app.workshop.toast {
                ui.weak(RichText::new(t).small());
            }
        });
        // C4d: AI 建/改流（需求输入+validate-loop 后台回路——结果出提议卡片）
        crate::ui::builder::builder_ui(app, ui);
        ui.separator();
        // tab 切换
        ui.horizontal(|ui| {
            ui.selectable_value(&mut app.workshop.tab, 0, "📝 代码");
            ui.selectable_value(&mut app.workshop.tab, 1, "🔬 试跑");
            ui.selectable_value(&mut app.workshop.tab, 2, "🗺 画布");
        });
        ui.separator();
        match app.workshop.tab {
            1 => {
                // 试跑视图：节点 trace 列表
                egui::ScrollArea::vertical()
                    .id_salt("ws_dry")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if app.workshop.trace.is_empty() && !app.workshop.dry_running {
                            ui.weak("（未试跑——点「▶ 试跑(Mock)」——零 token 走一遍节点顺序/变量传递/门禁分支）");
                        }
                        for r in &app.workshop.trace {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&r.node).strong().size(12.0));
                                ui.label(RichText::new(&r.event).small().color(Color32::from_rgb(140, 180, 240)));
                                ui.label(RichText::new(&r.detail).small().weak());
                            });
                        }
                    });
            }
            2 => {
                // C4e: 画布视图（只读图——拓扑分层/kind 配色/human_gate 标记——跟随草稿实时同步）
                crate::ui::canvas::canvas_ui(app, ui);
            }
            _ => {
                // 代码视图（JSON 编辑器——等宽——改即实时校验）
                egui::ScrollArea::vertical()
                    .id_salt("ws_code")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let response = ui.add(
                            egui::TextEdit::multiline(&mut app.workshop.draft)
                                .font(egui::TextStyle::Monospace)
                                .code_editor()
                                .desired_rows(28)
                                .desired_width(f32::INFINITY),
                        );
                        if response.changed() {
                            app.workshop.revalidate();
                        }
                    });
            }
        }
        // 底部校验状态条（Dify checklist 思想——错误清单可读）
        ui.separator();
        match &app.workshop.last_check {
            Some(Ok(())) => {
                ui.colored_label(Color32::from_rgb(110, 200, 120), "✅ 校验通过——可保存/可试跑");
            }
            Some(Err(errs)) => {
                ui.colored_label(
                    Color32::from_rgb(230, 110, 110),
                    RichText::new(format!("❌ {} 条错误:", errs.len())).strong(),
                );
                for e in errs.iter().take(6) {
                    ui.label(
                        RichText::new(format!("  · {e}"))
                            .small()
                            .color(Color32::from_rgb(230, 140, 140)),
                    );
                }
                if errs.len() > 6 {
                    ui.label(
                        RichText::new(format!("  … 共 {} 条", errs.len()))
                            .weak()
                            .small(),
                    );
                }
            }
            None => {}
        }
    });
}
