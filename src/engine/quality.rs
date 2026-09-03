//! engine/quality.rs — quality_check 质量评分（T4-3——2026-09-03）
//! Web 版 engine.py quality_check(53-107) 移植——4 维评分（一致性30/完整性25/创意性25/可实现性20）
//! Web 版是未接线死代码（score 永 0——门禁永不触发）——Rust 版接线：
//! 块锁定后调 quality_check → total<60(D 级) → 重跑 ≤2（T4-3b——设计意图完成）

use crate::ai::AiMessage;
use crate::engine::discussion::{sys_msg, user_msg};
use std::collections::HashMap;

/// 质量评分结果
#[derive(Debug, Clone)]
pub struct QualityResult {
    pub scores: HashMap<String, i64>,
    pub total: i64,
    pub grade: String,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
    pub improvement: String,
}

/// 评分失败默认（C 级 60 分）
fn default_result() -> QualityResult {
    let mut scores = HashMap::new();
    scores.insert("一致性".into(), 3);
    scores.insert("完整性".into(), 3);
    scores.insert("创意性".into(), 3);
    scores.insert("可实现性".into(), 3);
    QualityResult {
        scores,
        total: 60,
        grade: "C".into(),
        strengths: vec![],
        weaknesses: vec!["AI 评分解析失败，使用默认评分".into()],
        improvement: String::new(),
    }
}

/// 质量等级（Web quality_grade 移植）
pub fn quality_grade(score: i64) -> String {
    if score >= 90 {
        "A".into()
    } else if score >= 75 {
        "B".into()
    } else if score >= 60 {
        "C".into()
    } else {
        "D".into()
    }
}

/// 4 维质量评分（AI 调用——JSON 输出解析——失败默认 C 60）
pub async fn quality_check(
    ai: &dyn crate::ai::AiProvider,
    block_name: &str,
    field_values: &HashMap<String, String>,
    locked_context: &str,
) -> Result<QualityResult, String> {
    let fv_json = serde_json::to_string(field_values).unwrap_or_default();
    let prompt = format!(
        "你是一位专业的小说编辑和文学评论家。请对以下小说创作设定进行质量评估。

【当前创作的 Block】{block_name}
【设定内容】
{fv_json}

【已锁定的上文设定】
{}

请从以下 4 个维度评分（每个 1-5 分），并给出总分(加权后 0-100)：
1. 一致性（权重30%）：与已锁定设定无冲突，与类型和风格一致
2. 完整性（权重25%）：所有字段都有具体充实的内容
3. 创意性（权重25%）：不是通用模板，有独特的角度或细节
4. 可实现性（权重20%）：在篇幅内可展开，不会过于复杂难以把控

对于扣分项，务必指出具体问题在哪里。

输出格式（仅输出以下 JSON，不要其他内容）：
{{
  \"scores\": {{\"一致性\": 5, \"完整性\": 4, \"创意性\": 3, \"可实现性\": 4}},
  \"total\": 82,
  \"grade\": \"B\",
  \"strengths\": [\"优点1\", \"优点2\"],
  \"weaknesses\": [\"缺点1\", \"缺点2\"],
  \"improvement_suggestions\": \"如果你的评分低于 60 分（D级），请给出具体的改进方向，200字以内。\"
}}",
        if locked_context.is_empty() {
            "(无，这是第一个 Block)"
        } else {
            locked_context
        }
    );

    let reply = ai
        .chat(
            &[
                sys_msg("你是资深小说编辑，严格按输出格式返回 JSON。"),
                user_msg(&prompt),
            ],
            2048,
        )
        .await
        .map_err(|e| e.to_string())?;

    // 提取 JSON（Web: re.search(r'\{[\s\S]*\}')）
    let mut best: Option<QualityResult> = None;
    if let Some(start) = reply.content.find('{') {
        if let Some(end) = reply.content.rfind('}') {
            if end > start {
                let slice = &reply.content[start..=end];
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(slice) {
                    best = parse_json_result(&v);
                }
            }
        }
    }
    Ok(best.unwrap_or_else(|| {
        log::warn!("质量评分告警: block={block_name} AI 返回无法解析，使用默认 C 级 60 分。原始返回前200字: {}", &reply.content[..reply.content.len().min(200)]);
        default_result()
    }))
}

fn parse_json_result(v: &serde_json::Value) -> Option<QualityResult> {
    let total = v.get("total").and_then(|t| t.as_i64()).unwrap_or(60);
    let mut scores = HashMap::new();
    if let Some(s) = v.get("scores").and_then(|x| x.as_object()) {
        for (k, val) in s {
            if let Some(n) = val.as_i64() {
                scores.insert(k.clone(), n);
            }
        }
    }
    let grade = v
        .get("grade")
        .and_then(|g| g.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| quality_grade(total));
    let strengths = v
        .get("strengths")
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let weaknesses = v
        .get("weaknesses")
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let improvement = v
        .get("improvement_suggestions")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();
    Some(QualityResult {
        scores,
        total,
        grade,
        strengths,
        weaknesses,
        improvement,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::mock::MockProvider;
    use crate::engine::discussion::BoxAi;

    #[tokio::test]
    async fn quality_parses_grade() {
        let mut fv = HashMap::new();
        fv.insert("故事核".into(), "少年穿越仙侠".into());
        let script = vec![
            r#"{"scores":{"一致性":5,"完整性":4,"创意性":3,"可实现性":4},"total":82,"grade":"B","strengths":["创意好"],"weaknesses":["细节少"],"improvement_suggestions":"补充"}"#,
        ];
        let refs: Vec<&str> = script.iter().map(|s| *s).collect();
        let ai: BoxAi = Box::new(MockProvider::new(refs));
        let r = quality_check(&*ai, "故事核", &fv, "").await.unwrap();
        assert_eq!(r.total, 82);
        assert_eq!(r.grade, "B");
        assert_eq!(r.scores.get("一致性"), Some(&5));
        assert_eq!(r.strengths, vec!["创意好".to_string()]);
    }

    #[tokio::test]
    async fn quality_fallback_default() {
        let mut fv = HashMap::new();
        fv.insert("故事核".into(), "x".into());
        // mock 返回非 JSON——应默认 C 60
        let script = vec!["这不是JSON内容，随便说说"];
        let refs: Vec<&str> = script.iter().map(|s| *s).collect();
        let ai: BoxAi = Box::new(MockProvider::new(refs));
        let r = quality_check(&*ai, "故事核", &fv, "").await.unwrap();
        assert_eq!(r.total, 60);
        assert_eq!(r.grade, "C");
        assert!(r.weaknesses[0].contains("解析失败"));
    }

    #[test]
    fn grade_thresholds() {
        assert_eq!(quality_grade(95), "A");
        assert_eq!(quality_grade(80), "B");
        assert_eq!(quality_grade(65), "C");
        assert_eq!(quality_grade(30), "D");
    }
}
