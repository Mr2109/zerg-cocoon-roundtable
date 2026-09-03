//! 圆桌派（zerg-roundtable）——群 AI 讨论平台（zerg-cocoon 第一个茧）
//! 架构：引擎/模板分离——通用讨论引擎 + 项目模板（小说=13 Block 集）
//! 设计：虫族主 git docs/项目文档/v2.5.8/设计-v2.5.8-集装箱-圆桌派Rust重写-20260903.md

pub mod ai;      // AI 层（trait AiProvider——mock/虫族网关）
pub mod db;      // 数据层（rusqlite——线程隔离）
pub mod embed;   // 向量记忆（fastembed——T5）
pub mod engine;  // 讨论引擎（通用状态机——T4）
pub mod templates; // 项目模板（小说 BLOCKS——T4-1）
pub mod ui;      // egui 视图（T7）

/// 集装箱版本（cocoon 挂载标识）
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
