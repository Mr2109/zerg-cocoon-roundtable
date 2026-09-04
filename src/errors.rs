//! errors.rs — 统一错误枚举 RtError（L3——2026-09-04 设计定稿）
//! 四分类决定处理策略（对齐虫族故障自愈铁律）：
//!   User      用户操作错——不重试，UI 蓝色提示
//!   Env       环境故障——自动重试/退避，不计任务失败
//!   Bug       系统 bug——不重试，ERROR + errors 表
//!   AiQuality AI 质量问题——门禁 D 级重跑路径
//! 错误码四段：U1xx 用户 / E1xx 环境 / B1xx 系统 / Q1xx AI 质量

use std::fmt;

#[derive(Debug, Clone)]
pub enum RtError {
    /// 用户操作错——不重试，UI 提示改操作
    User { code: &'static str, msg: String },
    /// 环境故障——retryable=true 自动重试（AI 429/超时/DB busy）
    Env {
        code: &'static str,
        msg: String,
        retryable: bool,
    },
    /// 系统 bug——不重试，ERROR + 建议报 bug
    Bug { code: &'static str, msg: String },
    /// AI 质量问题——走 quality_check D 级重跑路径
    AiQuality { code: &'static str, msg: String },
}

// ── 错误码常量（抛出点引用——避免魔串）──
pub const U101_TOPIC_EMPTY: &str = "U101";
pub const U102_FIELD_LOCKED: &str = "U102";
pub const E101_AI_TIMEOUT: &str = "E101";
pub const E102_HTTP_429: &str = "E102";
pub const E103_DB_BUSY: &str = "E103";
pub const E104_FASTEMBED_MISSING: &str = "E104";
pub const B101_INVALID_STATE: &str = "B101";
pub const B102_LOCK_POISONED: &str = "B102";
pub const B103_JSON_PARSE: &str = "B103";
pub const Q101_SCORE_D: &str = "Q101";
pub const Q102_DRAFT_PARSE: &str = "Q102";
pub const Q103_EMPTY_REPLY: &str = "Q103";

impl RtError {
    pub fn code(&self) -> &'static str {
        match self {
            RtError::User { code, .. }
            | RtError::Env { code, .. }
            | RtError::Bug { code, .. }
            | RtError::AiQuality { code, .. } => code,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            RtError::User { .. } => "user",
            RtError::Env { .. } => "env",
            RtError::Bug { .. } => "bug",
            RtError::AiQuality { .. } => "ai_quality",
        }
    }

    pub fn msg(&self) -> &str {
        match self {
            RtError::User { msg, .. }
            | RtError::Env { msg, .. }
            | RtError::Bug { msg, .. }
            | RtError::AiQuality { msg, .. } => msg,
        }
    }

    /// 是否可自动重试（只有 Env{retryable:true}）
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            RtError::Env {
                retryable: true,
                ..
            }
        )
    }

    /// 上下文包装——保留原错误，附位置信息（detail 链）
    pub fn ctx(self, where_: &str) -> Self {
        match self {
            RtError::User { code, msg } => RtError::User {
                code,
                msg: format!("{msg} [{where_}]"),
            },
            RtError::Env {
                code,
                msg,
                retryable,
            } => RtError::Env {
                code,
                msg: format!("{msg} [{where_}]"),
                retryable,
            },
            RtError::Bug { code, msg } => RtError::Bug {
                code,
                msg: format!("{msg} [{where_}]"),
            },
            RtError::AiQuality { code, msg } => RtError::AiQuality {
                code,
                msg: format!("{msg} [{where_}]"),
            },
        }
    }
}

impl fmt::Display for RtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code(), self.msg())
    }
}

impl std::error::Error for RtError {}

/// 兼容垫：旧 String 错误迁移期 From<String>（归 Bug——L5 逐文件消掉后此实现可删）
impl From<String> for RtError {
    fn from(s: String) -> Self {
        RtError::Bug {
            code: B103_JSON_PARSE,
            msg: s,
        }
    }
}

impl From<&str> for RtError {
    fn from(s: &str) -> Self {
        RtError::from(s.to_string())
    }
}

/// AiError → RtError 映射（L4 zerg.rs 出口用——Env 按可重试 HTTP 状态归类）
impl From<crate::ai::AiError> for RtError {
    fn from(e: crate::ai::AiError) -> Self {
        use crate::ai::AiError as E;
        match &e {
            E::Http(s) => {
                // 429/5xx/连接类→可重试环境故障；其余 4xx→不可重试环境故障
                let retryable = s.contains("429")
                    || s.contains("500")
                    || s.contains("502")
                    || s.contains("503")
                    || s.contains("timed out")
                    || s.contains("connection");
                RtError::Env {
                    code: E102_HTTP_429,
                    msg: e.to_string(),
                    retryable,
                }
            }
            E::Api(s) => {
                if s.contains("401") || s.contains("404") || s.contains("400") {
                    RtError::Env {
                        code: "E105",
                        msg: e.to_string(),
                        retryable: false,
                    }
                } else {
                    RtError::Env {
                        code: E102_HTTP_429,
                        msg: e.to_string(),
                        retryable: true,
                    }
                }
            }
            E::Parse(s) => RtError::AiQuality {
                code: Q102_DRAFT_PARSE,
                msg: e.to_string(),
            },
            E::Empty => RtError::AiQuality {
                code: Q103_EMPTY_REPLY,
                msg: e.to_string(),
            },
            _ => RtError::Env {
                code: E101_AI_TIMEOUT,
                msg: e.to_string(),
                retryable: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classification() {
        let e = RtError::User {
            code: U101_TOPIC_EMPTY,
            msg: "主题为空".into(),
        };
        assert_eq!((e.code(), e.kind(), e.retryable()), ("U101", "user", false));

        let e = RtError::Env {
            code: E102_HTTP_429,
            msg: "429".into(),
            retryable: true,
        };
        assert!(e.retryable());
        assert_eq!(e.kind(), "env");
    }

    #[test]
    fn test_ctx_chain() {
        let e = RtError::Bug {
            code: B101_INVALID_STATE,
            msg: "非法迁移".into(),
        }
        .ctx("block:核心冲突/phase:讨论");
        assert!(e.to_string().contains("[block:核心冲突/phase:讨论]"));
        assert_eq!(e.code(), "B101"); // code 不被 ctx 改
    }

    #[test]
    fn test_from_string_compat() {
        let e: RtError = "旧式字符串错误".into();
        assert_eq!(e.kind(), "bug");
    }

    #[test]
    fn test_ai_error_map() {
        let e: RtError = crate::ai::AiError::Empty.into();
        assert_eq!(e.code(), "Q103");
        let e: RtError = crate::ai::AiError::Http("HTTP 429".into()).into();
        assert!(e.retryable());
    }
}
