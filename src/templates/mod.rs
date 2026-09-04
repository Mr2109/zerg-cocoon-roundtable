//! templates 模块 — 项目模板（T4-1——引擎/模板分离——内容层）
//! v1.0.1 A1: loader——flow.json 装载器（schema/变量/图三重校验——错误清单一次返回）

pub mod loader;
pub mod novel;

pub use loader::{get_template_loaded, load_flow_file, load_flow_str};
pub use novel::{
    default_templates_dir, find_block, get_template, Author, Block, GateRule, Moderator,
    ProjectTemplate, TemplateInput, NOVEL_BLOCKS,
};
