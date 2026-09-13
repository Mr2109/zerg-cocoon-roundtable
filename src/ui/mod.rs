//! ui/mod.rs — egui 0.36 界面（T7——2026-09-03）
//! 视图：Home（会话列表+新建）/Session（三栏讨论——T7-2）/Chapter（章节——T7-3）
//! egui 0.36 API：App trait 方法 ui()——面板 Panel::top/left/right + CentralPanel——show(ui)
//! 中文字体：复用虫族 UI 方案（PingFangSC 独立 TTF 优先）

use eframe::egui;
use egui::{Color32, RichText};

use crate::db::models::Session;
use crate::db::pool::Db;
use crate::ui::session::SessionRuntime;
use std::collections::HashMap;
use std::sync::atomic::Ordering;

pub mod builder;
pub mod canvas;
pub mod chapter;
pub mod collab;
pub mod session;
pub mod workshop;

/// AI 是否连接（env YZ_GATEWAY_TOKEN 有效——空/占位 ***/过短=未连接→AI mock——讨论/生成会空跑）——2026-09-03
pub fn ai_connected() -> bool {
    let t = std::env::var("YZ_GATEWAY_TOKEN").unwrap_or_default();
    let t = t.trim();
    !t.is_empty() && t != "***" && t.len() >= 8
}

/// mock 警示（无 token 时顶栏红字——防"空跑像真跑"误判）
pub fn ai_warn(ui: &mut egui::Ui) {
    if !ai_connected() {
        ui.label(
            RichText::new("⚠ AI=空跑（无 YZ_GATEWAY_TOKEN——mock 回复——非真实讨论）")
                .small()
                .color(Color32::from_rgb(230, 150, 90)),
        );
    }
}

/// 应用状态
pub struct RoundtableApp {
    pub db: Db,
    pub rt: tokio::runtime::Runtime,
    pub view: View,
    pub sessions: Vec<Session>,
    /// 会话后台运行状态
    pub runtimes: HashMap<String, SessionRuntime>,
    // 新建表单
    pub topic: String,
    pub novel_type: String,
    pub length: String,
    pub provider: String,
    pub err: Option<String>,
    /// 讨论流消息字号（px——A−/A+ 调节——2026-09-03）
    pub msg_px: f32,
    /// 章节视图：选中章 + 后台任务忙标志
    pub sel_chapter_id: i64,
    pub gen_busy: bool,
    pub _outline_done: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub _content_done: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// 面包屑：嵌入虫族 UI 时=true（显示"← 虫茧平台"——独立跑=false 无此级——2026-09-04 三层收一层）
    pub embedded: bool,
    /// 面包屑请求位：宿主每帧 render 后检查——true 则 rt_active=false 退出回平台栅格
    pub exit_platform: bool,
    /// 日志对话框开关（面包屑「📋 日志」——Mr2109 2026-09-04）
    pub show_log_window: bool,
    /// 日志对话框行缓存（异步加载——避免每帧查库）
    pub log_rows: Vec<crate::db::crud::ErrorRow>,
    /// 日志加载时间戳（防抖——打开时/手动刷新才拉）
    pub log_loaded_at: std::time::Instant,
    /// A4: 新建表单动态输入声明（当前模板 inputs——模板切换时刷新）
    pub form_inputs: Vec<crate::templates::TemplateInput>,
    /// A4: 新建表单动态值（key→用户输入/默认值）
    pub form_values: std::collections::HashMap<String, String>,
    /// B4: 可选模板列表 (id, name)——扫描 templates/*.flow.json
    pub available_templates: Vec<(String, String)>,
    /// B4: 当前选中的模板 id
    pub form_template: String,
    /// C4a: 工坊状态（人机共用任务流工作台）
    pub workshop: crate::ui::workshop::Workshop,
    /// C4a: 视图开关（true=显示工坊——false=圆桌派主界面）
    pub show_workshop: bool,
    /// C4c: 协作者状态（AI 提议卡片+变更流水）
    pub collab: crate::ui::collab::Collab,
    /// C4d: AI 建流回路状态
    pub flow_builder: crate::ui::builder::Builder,
}

#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Home,
    Session(String),
    Chapter(String),
}

const NOVEL_TYPES: [&str; 6] = ["玄幻", "都市", "科幻", "历史", "悬疑", "言情"];
/// 模型候选（网关 8082 实测可用——创作主力——2026-09-04 模型选择器）
pub(crate) const MODELS: [&str; 5] = [
    "ornith-1.5-35b",     // 默认——X3 中文好/工具稳/31tok/s
    "Qwen3.8-27B",        // 中文强思考
    "deepseek-v4-flash",  // 最强推理（加载慢）
    "Qwen3.8-Flash-Next", // 快速档
    "GLM-4.7-Flash",      // 备用
];

/// 状态中文映射（DB 值→界面文案——2026-09-04 细节完善）
pub(crate) fn status_cn(st: &str) -> String {
    match st {
        "running" => "运行中".into(),
        "completed" => "已完成".into(),
        "idle" => "待开始".into(),
        "failed" => "失败".into(),
        "stopped" => "已停止".into(),
        "awaiting_human" => "待确认".into(),
        _ => st.to_string(),
    }
}

impl RoundtableApp {
    pub fn new() -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let db = rt
            .block_on(Db::open(default_db_path()))
            .unwrap_or_else(|e| {
                eprintln!("DB 打开失败: {e}"); // 无 DB 无法运行——初始化失败 panic 合理（C 档保留）
                std::process::exit(1);
            });
        // 日志系统初始化（L1——DB 同目录 logs/——幂等——JSONL 15 天）
        crate::logging::init(
            &std::path::Path::new(&default_db_path())
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_default(),
        );
        let mut app = RoundtableApp {
            db,
            rt,
            view: View::Home,
            sessions: Vec::new(),
            runtimes: HashMap::new(),
            topic: String::new(),
            novel_type: "玄幻".into(),
            length: "长篇".into(),
            provider: "ornith-1.5-35b".into(),
            err: None,
            msg_px: 13.0,
            sel_chapter_id: -1,
            gen_busy: false,
            _outline_done: None,
            _content_done: None,
            embedded: false,
            exit_platform: false,
            show_log_window: false,
            log_rows: Vec::new(),
            log_loaded_at: std::time::Instant::now() - std::time::Duration::from_secs(3600),
            form_inputs: Vec::new(),
            form_values: Default::default(),
            available_templates: Vec::new(),
            form_template: "novel".into(),
            workshop: crate::ui::workshop::Workshop::new(),
            show_workshop: false,
            collab: crate::ui::collab::Collab::new(),
            flow_builder: crate::ui::builder::Builder::new(),
        };
        app.reset_stale_running(); // 上轮进程残留 running→idle（断点可重开——2026-09-03）
        app.refresh_sessions();
        // A4: 表单 inputs 从当前模板装载（novel 起步——未来模板切换时刷新）
        let t = crate::templates::loader::get_template_loaded(
            "novel",
            &crate::templates::default_templates_dir(),
        );
        app.form_inputs = t.inputs.clone();
        // B4: 扫描可选模板（templates/*.flow.json）+ novel 内置兜底
        app.available_templates = scan_templates();
        app
    }

    /// 复位残留 running（UI 进程被 kill 后引擎线程死——DB 残留 running——复位 idle 防假运行）
    fn reset_stale_running(&mut self) {
        let sids: Vec<(String, i64)> = match self.rt.block_on(self.db.get_all_sessions()) {
            Ok(v) => v
                .iter()
                .filter(|s| s.status == "running")
                .map(|s| (s.id.clone(), s.current_block))
                .collect(),
            Err(_) => return,
        };
        for (sid, blk) in sids {
            let _ = self
                .rt
                .block_on(self.db.update_session_progress(&sid, blk, "idle"));
            log::info!("残留 running 复位 idle: {} (block {})", sid, blk);
        }
    }

    /// 刷新会话列表
    pub fn refresh_sessions(&mut self) {
        match self.rt.block_on(self.db.get_all_sessions()) {
            Ok(v) => self.sessions = v,
            Err(e) => self.err = Some(format!("读会话失败: {e}")),
        }
    }

    /// 新建会话（入库——进入 Session 视图）
    pub fn create(&mut self) {
        let topic = self.topic.trim().to_string();
        if topic.is_empty() {
            self.err = Some("主题不能为空".into());
            return;
        }
        let (nt, len, prov) = (
            self.novel_type.clone(),
            // A4: 动态表单值优先——form_values["length"]（模板 inputs 渲染），回退旧固定字段
            self.form_values
                .get("length")
                .cloned()
                .unwrap_or_else(|| self.length.clone()),
            self.provider.clone(),
        );
        let id = format!("rt_{}", chrono_now());
        // B4: 用选中的模板——project_type 决定引擎装载哪份 flow
        let ptype = self.form_template.clone();
        let r = self.rt.block_on(
            self.db
                .create_session(&id, &topic, &nt, &len, &prov, &ptype),
        );
        match r {
            Ok(()) => {
                // A4: 表单动态值写入变量池（{{input.xxx}} 替换源——contract_text 等大文本走这里）
                let db = self.db.clone();
                let id2 = id.clone();
                let vals = self.form_values.clone();
                self.rt.block_on(async move {
                    for (k, v) in &vals {
                        let _ = db.set_flow_var(&id2, "input", k, v).await;
                    }
                });
                self.topic.clear();
                self.form_values.clear();
                self.err = None;
                self.refresh_sessions();
                self.view = View::Session(id);
            }
            Err(e) => self.err = Some(format!("新建失败: {e}")),
        }
    }
}

impl eframe::App for RoundtableApp {
    /// egui 0.36：App 方法为 ui()（非 update）——壳——渲染委托 render
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render(ui);
    }
}

impl RoundtableApp {
    /// 纯渲染（不依赖 eframe 壳——虫族 UI 虫茧直接调用嵌入）
    pub fn render(&mut self, ui: &mut egui::Ui) {
        self.exit_platform = false; // 每帧重置——面包屑点击时置 true——宿主本帧末读取
                                    // 统一面包屑顶栏（三层收一层——2026-09-04）:
                                    // 嵌入: ← 虫茧平台 | ← 圆桌派 | 会话标题（类型·字数·模型）| 📖章节 | 状态 | 停止/开始
                                    // 独立: 圆桌派 | 会话列表 | 提示
        let sess_info: Option<(String, String, String, String)> = match &self.view {
            View::Session(sid) | View::Chapter(sid) => self
                .rt
                .block_on(self.db.get_session(sid))
                .ok()
                .flatten()
                .map(|s| {
                    (
                        s.name.clone(),
                        s.novel_type.clone(),
                        s.length,
                        s.provider.clone(),
                        s.status.clone(),
                    )
                })
                .map(|(n, t, l, p, st)| (n, format!("{}·{}", t, p), l, st)),
            _ => None,
        };
        egui::Panel::top("rt_top").show(ui, |ui| {
            ui.horizontal(|ui| {
                if self.embedded {
                    if ui.button("← 虫茧平台").clicked() {
                        self.exit_platform = true;
                    }
                    ui.separator();
                    if ui.button("← 圆桌派").clicked() {
                        self.view = View::Home;
                        self.refresh_sessions();
                    }
                } else {
                    ui.heading(RichText::new("圆桌派").color(Color32::from_rgb(200, 160, 60)));
                    if ui.button("会话").clicked() {
                        self.view = View::Home;
                        self.refresh_sessions();
                    }
                }
                ui.separator();
                // 会话级面包屑（Session/Chapter 视图——标题+元信息+状态+操作全在这一条）
                if let Some((name, meta, _len, status)) = &sess_info {
                    let st_color = match status.as_str() {
                        "completed" => Color32::from_rgb(110, 200, 120),
                        "running" => Color32::from_rgb(220, 180, 90),
                        _ => Color32::GRAY,
                    };
                    ui.heading(name.clone());
                    ui.label(RichText::new(format!("（{}）", meta)).weak());
                    ui.separator();
                    // 视图相关跳转：Session→章节页；Chapter→返回会话
                    match &self.view {
                        View::Session(_) => {
                            if ui.button("📖 章节").clicked() {
                                if let View::Session(sid) = self.view.clone() {
                                    self.view = View::Chapter(sid);
                                }
                            }
                        }
                        View::Chapter(_) => {
                            if ui.button("← 返回会话").clicked() {
                                if let View::Chapter(sid) = self.view.clone() {
                                    self.view = View::Session(sid);
                                }
                            }
                        }
                        _ => {}
                    }
                    ui.separator();
                    ui.label(RichText::new(format!("状态: {}", status_cn(status))).color(st_color));
                    self.session_top_controls(ui, status);
                } else {
                    ui.label(
                        RichText::new("群 AI 讨论——小说生成只是它的一个项目")
                            .weak()
                            .small(),
                    );
                }
                ui.separator();
                // 日志按钮（面包屑一级入口——Mr2109 2026-09-04——全局可见）
                if ui.button("📋 日志").clicked() {
                    self.show_log_window = !self.show_log_window;
                    self.log_loaded_at =
                        std::time::Instant::now() - std::time::Duration::from_secs(3600);
                    // 触发重载
                }
                // C4a: 工坊入口（人机共用任务流工作台）
                if ui.button("🛠 工坊").clicked() {
                    self.show_workshop = !self.show_workshop;
                    if self.show_workshop && self.available_templates.is_empty() {
                        self.available_templates = scan_templates();
                    }
                }
                if self.embedded {
                    ui.separator();
                    ui.label(
                        RichText::new("群 AI 讨论——小说生成只是它的一个项目")
                            .weak()
                            .small(),
                    );
                }
            });
        });
        // 视图主体
        if self.show_workshop {
            crate::ui::workshop::workshop_view(self, ui);
            // 工坊内保存/新建后刷新模板列表（新建流立即可被新建表单选到）
            let count = self.available_templates.len();
            self.available_templates = scan_templates();
            let _ = count;
        } else {
            match self.view.clone() {
                View::Home => self.home_view(ui),
                View::Session(sid) => crate::ui::session::session_view(self, ui, &sid),
                View::Chapter(sid) => crate::ui::chapter::chapter_view(self, ui, &sid),
            }
        }
        // 日志对话框（独立 egui Window——不挤三栏——打开时/60s 防抖重载）
        if self.show_log_window {
            let reload = self.log_loaded_at.elapsed() > std::time::Duration::from_secs(60);
            if reload {
                self.log_loaded_at = std::time::Instant::now();
                // 同步轻查询（200 行以内毫秒级——UI 卡顿可忽略；重查走 spawn_blocking 线程池）
                let rows = self
                    .rt
                    .block_on(self.db.list_errors(None, 200))
                    .unwrap_or_default();
                self.log_rows = rows;
            }
            let mut open = self.show_log_window;
            egui::Window::new("📋 报错与日志（最新 200 条 WARN/ERROR——保留 15 天）")
                .open(&mut open)
                .default_width(860.0)
                .default_height(520.0)
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("🔄 刷新").clicked() {
                            self.log_loaded_at =
                                std::time::Instant::now() - std::time::Duration::from_secs(3600);
                        }
                        ui.weak(format!("{} 条", self.log_rows.len()));
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if self.log_rows.is_empty() {
                                ui.weak("（无 WARN/ERROR 记录——一切正常）");
                            }
                            egui::Grid::new("log_grid")
                                .striped(true)
                                .num_columns(5)
                                .show(ui, |ui| {
                                    for r in &self.log_rows {
                                        let lv_color = if r.level == "ERROR" {
                                            Color32::from_rgb(230, 110, 110)
                                        } else {
                                            Color32::from_rgb(230, 160, 90)
                                        };
                                        ui.label(RichText::new(&r.ts).weak().small());
                                        ui.label(
                                            RichText::new(&r.level)
                                                .color(lv_color)
                                                .strong()
                                                .small(),
                                        );
                                        ui.label(
                                            RichText::new(format!("{} {}", r.kind, r.module))
                                                .small(),
                                        );
                                        ui.label(
                                            RichText::new(format!(
                                                "{}{}",
                                                r.code.as_deref().unwrap_or("—"),
                                                if r.sid.is_some() {
                                                    format!(" sid={}", r.sid.as_deref().unwrap())
                                                } else {
                                                    String::new()
                                                }
                                            ))
                                            .small(),
                                        );
                                        ui.label(RichText::new(&r.msg).size(12.0));
                                        ui.end_row();
                                        if let Some(d) = &r.detail {
                                            if !d.is_empty() {
                                                ui.label("");
                                                ui.label("");
                                                ui.label("");
                                                ui.label("");
                                                ui.label(
                                                    RichText::new(d)
                                                        .weak()
                                                        .small()
                                                        .text_style(egui::TextStyle::Monospace),
                                                );
                                                ui.end_row();
                                            }
                                        }
                                    }
                                });
                        });
                });
            self.show_log_window = open;
        }
    }

    /// 会话顶栏操作区（停止/开始讨论——从 session.rs 顶栏上移——含 runtime done 清理/ai_warn）
    fn session_top_controls(&mut self, ui: &mut egui::Ui, status: &str) {
        let sid = match &self.view {
            View::Session(sid) | View::Chapter(sid) => sid.clone(),
            _ => return,
        };
        // 引擎已退（停止/完成）——清理 runtime——按钮回"开始"
        if self
            .runtimes
            .get(&sid)
            .is_some_and(|r| r.done.load(Ordering::Relaxed))
        {
            self.runtimes.remove(&sid);
        }
        ui.separator();
        // 按会话 DB 状态判断（running 中显示停止；awaiting_human 显示确认卡）
        if status == "running" {
            if ui.button("停止").clicked() {
                self.stop_discussion(&sid);
            }
        } else if status == "awaiting_human" {
            // B3: 确认卡——批准/驳回（写 flow_vars 人工裁决 + 状态回 idle 引擎可续跑）
            ui.colored_label(
                Color32::from_rgb(230, 180, 90),
                RichText::new("⏸ 等待人工确认"),
            );
            if ui.button("✅ 批准").clicked() {
                self.human_decide(&sid, "通过");
            }
            if ui.button("❌ 驳回").clicked() {
                self.human_decide(&sid, "驳回");
            }
        } else {
            crate::ui::ai_warn(ui);
            if ui.button("▶ 开始讨论").clicked() {
                self.start_discussion(&sid);
            }
        }
    }

    /// B3: 人工裁决落变量池 + 会话状态回 idle（引擎续跑时 gate 读裁决走对应分支）
    fn human_decide(&mut self, sid: &str, verdict: &str) {
        // 当前挂起的节点=会话 current_block
        let (node_idx,) = match self.rt.block_on(self.db.get_session(sid)) {
            Ok(Some(s)) => (s.current_block,),
            _ => return,
        };
        let node_id = format!("n{node_idx}");
        let v = verdict.to_string();
        let db = self.db.clone();
        let sid2 = sid.to_string();
        let nid2 = node_id.clone();
        self.rt.block_on(async move {
            let _ = db.set_flow_var(&sid2, &nid2, "人工裁决", &v).await;
            let _ = db.update_session_progress(&sid2, node_idx, "idle").await;
        });
        log::info!("人工裁决: 会话 {sid} 节点 {node_id} → {verdict}");
    }
}

impl RoundtableApp {
    /// Home 视图：会话列表 + 新建表单
    fn home_view(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("home_left")
            .default_size(320.0)
            .show(ui, |ui| {
                ui.add_space(6.0);
                ui.heading("会话");
                ui.separator();
                if self.sessions.is_empty() {
                    ui.label(RichText::new("（无会话——右侧新建）").weak());
                }
                let mut open: Option<String> = None;
                for s in &self.sessions {
                    let status_color = match s.status.as_str() {
                        "completed" => Color32::from_rgb(110, 200, 120),
                        "running" => Color32::from_rgb(220, 180, 90),
                        _ => Color32::GRAY,
                    };
                    let label = format!(
                        "{}  [{}·{}·{}]",
                        s.name,
                        s.novel_type,
                        s.provider,
                        crate::ui::status_cn(&s.status)
                    );
                    if ui
                        .selectable_label(false, RichText::new(label).color(status_color))
                        .clicked()
                    {
                        open = Some(s.id.clone());
                    }
                    ui.label(
                        RichText::new(format!("Block {}/13", s.current_block))
                            .weak()
                            .small(),
                    );
                    ui.add_space(2.0);
                }
                if let Some(sid) = open {
                    self.view = View::Session(sid);
                }
            });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(8.0);
            ui.heading("新建项目（圆桌派讨论）");
            ui.separator();
            // B4: 模板选择（扫描 templates/*.flow.json——选择即刷新表单 inputs）
            ui.horizontal(|ui| {
                ui.label("任务流：");
                let avail = self.available_templates.clone();
                let cur = self.form_template.clone();
                egui::ComboBox::from_id_salt("tpl_select")
                    .selected_text(format!(
                        "{}",
                        avail
                            .iter()
                            .find(|(id, name)| id == &cur)
                            .map(|(_, n)| n.as_str())
                            .unwrap_or(&cur)
                    ))
                    .show_ui(ui, |ui| {
                        for (id, name) in &avail {
                            ui.selectable_value(&mut self.form_template, id.clone(), name);
                        }
                    });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("主题：");
                ui.add(
                    egui::TextEdit::singleline(&mut self.topic)
                        .hint_text("例：仙侠少年复仇记")
                        .desired_width(280.0),
                );
            });
            ui.add_space(6.0);
            // 非 novel 模板隐藏小说类型行（类型行是小说特有）
            if self.form_template == "novel" {
                ui.horizontal(|ui| {
                    ui.label("类型：");
                    for t in NOVEL_TYPES {
                        if ui.selectable_label(self.novel_type == t, t).clicked() {
                            self.novel_type = t.to_string();
                        }
                    }
                });
                ui.add_space(6.0);
            }
            // A4: inputs 动态渲染（模板声明驱动——不再写死篇幅行；novel 模板 inputs=length）
            // B4: 模板切换→刷新 inputs（form_values 清空防串模板）
            {
                let t = crate::templates::loader::get_template_loaded(
                    &self.form_template.clone(),
                    &crate::templates::default_templates_dir(),
                );
                let new_keys: Vec<String> = t.inputs.iter().map(|i| i.key.clone()).collect();
                let stale = self
                    .form_inputs
                    .iter()
                    .map(|i| i.key.clone())
                    .collect::<Vec<_>>()
                    != new_keys;
                if stale {
                    self.form_inputs = t.inputs.clone();
                    self.form_values.clear();
                }
            }
            for inp in &self.form_inputs {
                ui.horizontal(|ui| {
                    ui.label(format!("{}：", inp.label));
                    if inp.options.is_empty() {
                        // 自由文本输入
                        let buf = self
                            .form_values
                            .entry(inp.key.clone())
                            .or_insert_with(|| inp.default.clone().unwrap_or_default());
                        ui.add(
                            egui::TextEdit::singleline(buf)
                                .hint_text(&inp.label)
                                .desired_width(200.0),
                        );
                    } else {
                        let cur = self.form_values.entry(inp.key.clone()).or_insert_with(|| {
                            inp.default
                                .clone()
                                .or_else(|| inp.options.first().cloned())
                                .unwrap_or_default()
                        });
                        for opt in &inp.options {
                            if ui.selectable_label(*cur == *opt, opt).clicked() {
                                *cur = opt.clone();
                            }
                        }
                    }
                });
                ui.add_space(4.0);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("模型：");
                for m in MODELS {
                    if ui.selectable_label(self.provider == m, m).clicked() {
                        self.provider = m.to_string();
                    }
                }
            });
            ui.add_space(12.0);
            if ui.button(RichText::new("开始讨论").size(16.0)).clicked() {
                self.create();
            }
            if let Some(e) = &self.err {
                ui.colored_label(Color32::from_rgb(220, 100, 100), e);
            }
        });
    }
}

/// 默认库路径（圆桌派数据独立——zerg-cocoon/圆桌派/data/roundtable.db）
fn default_db_path() -> String {
    let mut p = std::env::current_dir().unwrap_or_else(|_| ".".into());
    p.push("data");
    std::fs::create_dir_all(&p).ok();
    p.push("roundtable.db");
    p.to_string_lossy().to_string()
}

/// B4: 扫描可选模板（templates/*.flow.json——id+name；novel 内置兜底排首）
fn scan_templates() -> Vec<(String, String)> {
    let mut out = vec![("novel".to_string(), "小说设定流水线".to_string())];
    let dir = crate::templates::default_templates_dir();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.ends_with(".flow.json") || name == "novel.flow.json" {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(e.path()) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let (Some(id), Some(fname)) = (
                        v.get("id").and_then(|x| x.as_str()),
                        v.get("name").and_then(|x| x.as_str()),
                    ) {
                        out.push((id.to_string(), fname.to_string()));
                    }
                }
            }
        }
    }
    out
}

/// 时间戳（会话 ID——非加密）
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{s:x}")
}

/// 中文字体（复用虫族 UI 经验：PingFangSC 独立 TTF 优先——动态字体包扫描兜底）
pub fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut loaded = false;
    for p in [
        // 公开仓不含任何用户家目录路径：只认系统字体位置（+ 运行目录 AssetData 动态扫描兜底）
        "/System/Library/Fonts/PingFang.ttc",
    ] {
        if std::path::Path::new(p).exists() {
            fonts.font_data.insert(
                "pingfang".into(),
                egui::FontData::from_owned(std::fs::read(p).unwrap()).into(),
            );
            fonts
                .families
                .get_mut(&egui::FontFamily::Proportional)
                .unwrap()
                .insert(0, "pingfang".into());
            fonts
                .families
                .get_mut(&egui::FontFamily::Monospace)
                .unwrap()
                .push("pingfang".into());
            loaded = true;
            break;
        }
    }
    if !loaded {
        if let Ok(rd) = std::fs::read_dir("/System/Library/AssetsV2/com_apple_MobileAsset_Font7") {
            for e in rd.flatten() {
                let ap = e.path().join("AssetData/PingFang.ttc");
                if ap.exists() {
                    fonts.font_data.insert(
                        "pingfang".into(),
                        egui::FontData::from_owned(std::fs::read(ap).unwrap()).into(),
                    );
                    fonts
                        .families
                        .get_mut(&egui::FontFamily::Proportional)
                        .unwrap()
                        .insert(0, "pingfang".into());
                    loaded = true;
                    break;
                }
            }
        }
    }
    ctx.set_fonts(fonts);
}
