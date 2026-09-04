//! ai/mod.rs — AI 层（T3——2026-09-03）
//! trait AiProvider 抽象：真 provider（zerg 虫族网关）/ mock provider（脚本化——测试确定性——M4）
//! Web 版 call_api 移植语义：非流式 chat completions——think 剥离 + 中文提取链

pub mod mock;
pub mod zerg;

use serde::{Deserialize, Serialize};

/// 对话消息（对齐 OpenAI chat completions 格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiMessage {
    pub role: String, // system / user / assistant
    pub content: String,
}

/// AI 回复
#[derive(Debug, Clone)]
pub struct AiReply {
    pub content: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
}

impl AiReply {
    pub fn total_tokens(&self) -> i64 {
        self.prompt_tokens + self.completion_tokens
    }
}

/// AI 提供者抽象（M4——引擎测试用 mock——真跑用 zerg——async-trait: dyn 安全）
#[async_trait::async_trait]
pub trait AiProvider: Send + Sync {
    /// 聊天调用（非流式——返回完整回复）
    async fn chat(&self, msgs: &[AiMessage], max_tokens: i64) -> Result<AiReply, AiError>;

    /// 提供者名（日志/记录）
    fn name(&self) -> &str;
}

#[derive(thiserror::Error, Debug)]
pub enum AiError {
    #[error("HTTP: {0}")]
    Http(String),
    #[error("API 错误: {0}")]
    Api(String),
    #[error("响应解析失败: {0}")]
    Parse(String),
    #[error("空回复（多次重试后）")]
    Empty,
}
