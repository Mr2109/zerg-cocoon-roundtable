//! engine 模块 — 讨论引擎（T4——通用——引擎/模板分离）
//! v1.0.1 B1: nodes——四类流节点 trait + single/gate 最小实现

pub mod chapters;
pub mod discussion;
#[cfg(test)]
pub mod flow_test;
pub mod memory;
pub mod nodes;
pub mod quality;
pub mod review;
pub mod run;
pub mod scheduler;
pub mod utils;
