//! templates 模块 — 项目模板（T4-1——引擎/模板分离——内容层）

pub mod novel;

pub use novel::{get_template, find_block, Author, Block, Moderator, ProjectTemplate, NOVEL_BLOCKS};
