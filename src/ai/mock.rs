//! ai/mock.rs — MockProvider（T3——2026-09-03——M4 测试确定性）
//! 脚本化响应队列：每次 chat 弹出一条预设响应——引擎单测用（快/确定/不烧 token）

use super::{AiError, AiMessage, AiProvider, AiReply};
use std::sync::{Arc, Mutex};

/// 脚本化 mock——按顺序返回预设内容（用完循环最后一条）
pub struct MockProvider {
    script: Arc<Mutex<std::collections::VecDeque<String>>>,
    fallback: String,
}

impl MockProvider {
    pub fn new(responses: Vec<&str>) -> Self {
        let q = responses
            .into_iter()
            .map(|s| s.to_string())
            .collect::<std::collections::VecDeque<_>>();
        MockProvider {
            script: Arc::new(Mutex::new(q)),
            fallback: "（mock 默认回复）".to_string(),
        }
    }

    /// 无脚本内容的默认 mock（引擎测试跑长流程用）
    pub fn default_reply(text: &str) -> Self {
        MockProvider::new(vec![text])
    }
}

#[async_trait::async_trait]
impl AiProvider for MockProvider {
    async fn chat(&self, _msgs: &[AiMessage], _max_tokens: i64) -> Result<AiReply, AiError> {
        let mut q = self
            .script
            .lock()
            .map_err(|e| AiError::Api(e.to_string()))?;
        let text = if q.len() > 1 {
            q.pop_front().unwrap_or_else(|| self.fallback.clone())
        } else {
            q.front().cloned().unwrap_or_else(|| self.fallback.clone())
        };
        let completion = text.chars().count() as i64;
        Ok(AiReply {
            content: text,
            prompt_tokens: 10,
            completion_tokens: completion,
        })
    }

    fn name(&self) -> &str {
        "mock"
    }
}
