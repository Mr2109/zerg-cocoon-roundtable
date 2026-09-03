//! ai/zerg.rs — ZergProvider（T3——2026-09-03）
//! 调虫族网关（127.0.0.1:8082/v1/chat/completions——chat 格式）——Web 版 call_api 移植
//! 语义链（照抄 Web 版）：
//! ① <think>...</think> 剥离取尾部 ② 思考不闭合→从首个中文密集段(中文≥40%且≥6字)取正文
//! ③ content 空→从 reasoning_content 提取（中文正则——尾部规则）
//! 凭据：token 走配置（env YZ_GATEWAY_TOKEN / YZ_GATEWAY_URL / YZ_GATEWAY_MODEL）——不入 git
//! 非流式（对齐 Web 版 stream=False）——流式 UI 打字机后续 T7 再扩展

use super::{AiError, AiMessage, AiProvider, AiReply};
use serde_json::json;

/// 虫族网关配置（env 可覆盖——默认 Web 版同款）
#[derive(Debug, Clone)]
pub struct ZergConfig {
    pub url: String,
    pub token: String,
    pub model: String,
}

impl Default for ZergConfig {
    fn default() -> Self {
        ZergConfig {
            url: std::env::var("YZ_GATEWAY_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8082/v1/chat/completions".to_string()),
            token: std::env::var("YZ_GATEWAY_TOKEN").unwrap_or_default(),
            model: std::env::var("YZ_GATEWAY_MODEL")
                .unwrap_or_else(|_| "ornith-1.5-35b".to_string()),
        }
    }
}

/// 真 provider——虫族网关
pub struct ZergProvider {
    cfg: ZergConfig,
    client: reqwest::Client,
}

impl ZergProvider {
    pub fn new(cfg: ZergConfig) -> Self {
        ZergProvider {
            cfg,
            client: reqwest::Client::builder()
                .no_proxy() // 禁代理——走直连（macOS 死系统代理 7890 会 Connection refused——2026-09-03 实证）
                .timeout(std::time::Duration::from_secs(310))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn from_env() -> Self {
        ZergProvider::new(ZergConfig::default())
    }

    /// 指定模型（模型选择器——会话级模型名——覆盖 env 默认）
    pub fn with_model(model: &str) -> Self {
        let mut cfg = ZergConfig::default();
        cfg.model = model.to_string();
        ZergProvider::new(cfg)
    }
}

#[async_trait::async_trait]
impl AiProvider for ZergProvider {
    async fn chat(&self, msgs: &[AiMessage], max_tokens: i64) -> Result<AiReply, AiError> {
        let mut payload = json!({
            "model": self.cfg.model,
            "messages": msgs,
            "temperature": 0.85,
            "stream": false,
        });
        if max_tokens > 0 {
            payload["max_tokens"] = json!(max_tokens);
        }
        // 3 次重试（对齐 Web 版 attempt 3——timeout 递增由 reqwest 总超时覆盖）
        let mut last_err: Option<AiError> = None;
        for attempt in 0..3 {
            let mut req = self
                .client
                .post(&self.cfg.url)
                .json(&payload)
                .header("Content-Type", "application/json");
            if !self.cfg.token.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", self.cfg.token));
            }
            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if !status.is_success() {
                        let body = resp.text().await.unwrap_or_default();
                        let e = AiError::Api(format!("HTTP {}: {}", status, body.chars().take(200).collect::<String>()));
                        // 5xx/429 重试——4xx 不重试
                        if attempt < 2 && resp_is_retryable(&e) {
                            last_err = Some(e);
                            tokio::time::sleep(std::time::Duration::from_secs(2 + attempt * 2)).await;
                            continue;
                        }
                        return Err(e);
                    }
                    let text = resp.text().await.map_err(|e| AiError::Http(e.to_string()))?;
                    let result: serde_json::Value =
                        serde_json::from_str(&text).map_err(|e| AiError::Parse(e.to_string()))?;
                    if let Some(err) = result.get("error") {
                        return Err(AiError::Api(err.to_string()));
                    }
                    let usage = &result["usage"];
                    let prompt = usage["prompt_tokens"].as_i64().unwrap_or(0);
                    let completion = usage["completion_tokens"].as_i64().unwrap_or(0);
                    let raw = result["choices"][0]["message"]["content"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    let reasoning = result["choices"][0]["message"]["reasoning_content"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    let content = clean_content(&raw, &reasoning);
                    if content.is_empty() {
                        // 重试一轮（空回复）
                        if attempt < 2 {
                            last_err = Some(AiError::Empty);
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            continue;
                        }
                        return Err(AiError::Empty);
                    }
                    return Ok(AiReply {
                        content,
                        prompt_tokens: prompt,
                        completion_tokens: completion,
                    });
                }
                Err(e) => {
                    let ae = AiError::Http(e.to_string());
                    last_err = Some(ae);
                    if attempt < 2 {
                        tokio::time::sleep(std::time::Duration::from_secs(2 + attempt * 2)).await;
                        continue;
                    }
                }
            }
        }
        Err(last_err.unwrap_or(AiError::Empty))
    }

    fn name(&self) -> &str {
        "zerg"
    }
}

fn resp_is_retryable(e: &AiError) -> bool {
    match e {
        AiError::Api(s) => {
            s.contains("429") || s.contains("502") || s.contains("503") || s.contains("500")
        }
        _ => false,
    }
}

/// 正文清理链（Web 版 call_api 移植——思考模型处理）
pub fn clean_content(raw: &str, reasoning: &str) -> String {
    let mut content = raw.trim().to_string();
    // ① <think>...</think> 剥离
    if content.contains("</think>") {
        if let Some(idx) = content.find("</think>") {
            content = content[idx + "</think>".len()..].trim().to_string();
        }
    }
    // ② 思考不闭合/混英文——从首个中文密集段开始
    if !content.is_empty() {
        let paras: Vec<&str> = content.split('\n').map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
        let mut start = -1i32;
        for (i, p) in paras.iter().enumerate() {
            let cn: usize = p.chars().filter(|c| *c as u32 >= 0x4e00 && *c as u32 <= 0x9fff).count();
            let no_ws: usize = p.chars().filter(|c| !c.is_whitespace()).count();
            if no_ws > 0 && (cn as f64 / no_ws as f64) >= 0.4 && cn >= 6 {
                start = i as i32;
                break;
            }
        }
        if start > 0 {
            content = paras[start as usize..].join("\n");
        }
    }
    // ③ content 空——从 reasoning_content 提取中文
    if content.is_empty() && !reasoning.is_empty() {
        content = extract_from_reasoning(reasoning);
    }
    content.trim().to_string()
}

/// 从 reasoning 提取中文正文（Web 版逻辑移植）
fn extract_from_reasoning(reasoning: &str) -> String {
    // 中文连续段（≥4 字）
    let re = regex::Regex::new(r"[\u4e00-\u9fff　-〿＀-￯]{4,}").unwrap();
    let matches: Vec<&str> = re.find_iter(reasoning).map(|m| m.as_str()).collect();
    if let Some(last) = matches.last() {
        return last.to_string();
    }
    // 无中文——取最后一句合理行（尾部规则精简版）
    let lines: Vec<&str> = reasoning.split('\n').map(|l| l.trim()).filter(|l| l.len() > 3).collect();
    for l in lines.iter().rev() {
        let bad_prefixes = [
            "Here", "1.", "2.", "3.", "4.", "5.", "6.", "7.", "8.", "9.", "0.", "- ", "**",
            "I need", "Let me", "The user", "This is", "Then ", "First ", "Finally", "Output:",
            "Input:", "Analyze", "Identify", "Formulate", "Verify", "Refine", "Check", "Execute",
            "Example", "Simpler", "Constraint", "Response:", "Think", "So,", "Therefore", "Thus",
            "In summary", "Based on", "Given that", "Note that",
        ];
        if !bad_prefixes.iter().any(|b| l.starts_with(b)) {
            let out = l.to_string();
            if out.ends_with(['。', '！', '？', '.']) {
                return out;
            }
            return out;
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_think_tag() {
        let r = clean_content("思考中…</think>正文开始", "");
        assert_eq!(r, "正文开始");
    }

    #[test]
    fn clean_unclosed_think_uses_chinese_para() {
        let raw = "Let me think about the worldbuilding carefully\nThe 主角穿越到仙侠大陆，这里灵气复苏，各方势力蠢蠢欲动。他需要在这个世界生存下去。";
        let r = clean_content(raw, "");
        assert!(r.contains("主角穿越"));
        assert!(!r.contains("Let me"));
    }

    #[test]
    fn clean_empty_content_uses_reasoning() {
        let r = clean_content("", "This is reasoning in English about the setting\n最终决定世界设定为九州大陆，灵气复苏时代，宗门林立。");
        assert!(r.contains("九州大陆"));
    }

    /// 真调验证（T3-2 验收——需 env: YZ_GATEWAY_TOKEN=xxx 且网关在线——cargo test --ignored）
    #[tokio::test]
    #[ignore = "真调网关——需凭据与 ornith 在线"]
    async fn live_gateway_call() {
        let p = ZergProvider::from_env();
        assert!(!p.cfg.token.is_empty(), "需 YZ_GATEWAY_TOKEN");
        let reply = p
            .chat(&[AiMessage { role: "user".into(), content: "用一句话回答：1+1=？".into() }], 200)
            .await
            .expect("网关调用成功");
        println!("真调回复: {}", reply.content);
        assert!(!reply.content.is_empty());
    }
}
