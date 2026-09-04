//! ui/workshop.rs — 工坊（C4a——人机共用任务流工作台）
//! 布局：模板库(左) | 工作区(中——代码视图) | 校验状态条(底部)
//! 铁律：所有修改走装载器校验（唯一裁判）——校验过才可保存
//! Dify 对标：panel-slice 双区布局 / checklist 校验清单 / draft 思想（未保存=草稿态）

use eframe::egui;
use egui::{Color32, RichText};

use crate::templates::loader;

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
}

impl Workshop {
    pub fn new() -> Self {
        Workshop {
            editing: None,
            draft: String::new(),
            last_check: None,
            saved_baseline: None,
            toast: None,
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
}

/// 工坊主视图（三区——左模板库/中代码/底校验条）
pub fn workshop_view(app: &mut crate::ui::RoundtableApp, ui: &mut egui::Ui) {
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

    // 中：代码视图 + 底部校验条
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
            if ui
                .add_enabled(dirty, egui::Button::new("💾 保存"))
                .clicked()
            {
                app.workshop
                    .save(&crate::templates::default_templates_dir());
            }
            if ui.button("↺ 重载文件").clicked() {
                if let Some(id) = app.workshop.editing.clone() {
                    app.workshop
                        .open(&id, &crate::templates::default_templates_dir());
                }
            }
            if let Some(t) = &app.workshop.toast {
                ui.weak(RichText::new(t).small());
            }
        });
        ui.separator();
        // JSON 编辑器（等宽——TextEdit codeeditor）
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
                    app.workshop.revalidate(); // 实时校验（装载器毫秒级——直接同步调）
                }
            });
        // 底部校验状态条（Dify checklist 思想——错误清单可读）
        ui.separator();
        match &app.workshop.last_check {
            Some(Ok(())) => {
                ui.colored_label(Color32::from_rgb(110, 200, 120), "✅ 校验通过——可保存");
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
