//! engine/utils.rs — 引擎辅助函数（T4-2——2026-09-03——Web 版 engine.py 207-304 移植）
//! parse_fields / pick_draft / has_satisfied / extract_draft_segment / draft_bigram_overlap

/// 解析字段值（字段名:值——JSON 优先——失败回退文本——Web parse_fields 移植）
pub fn parse_fields(text: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let text = text.trim();
    if text.is_empty() {
        return out;
    }
    // JSON 优先
    if text.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
            if let Some(obj) = v.as_object() {
                for (k, val) in obj {
                    if !val.is_null() {
                        out.insert(k.clone(), val.as_str().unwrap_or("").to_string());
                    }
                }
                return out;
            }
        }
    }
    // 文本解析（每行 key:value——跳过噪音 key）
    let skip_keys = [
        "格式", "字段", "字段列表", "讨论摘要", "基于以下讨论", "当前方案", "讨论意见", "格式示例",
    ];
    for line in text.split('\n') {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        for sep in ["：", ":", "="] {
            if let Some(idx) = line.find(sep) {
                let k = line[..idx].trim().trim_start_matches('#').trim().to_string();
                let v = line[idx + sep.len()..].trim().to_string();
                if !k.is_empty() && !v.is_empty() && !skip_keys.contains(&k.as_str()) {
                    out.insert(k, v);
                    break;
                }
            }
        }
    }
    out
}

/// 从多草案文本中选出 wd 号草案（Web pick_draft 移植）
pub fn pick_draft(text: &str, wd: usize) -> String {
    let text = text.trim();
    if text.is_empty() || text == "_" {
        return String::new();
    }
    // 按 "草案N：" 分割
    let re = regex::Regex::new(r"\s*草案\d[：:]\s*").unwrap();
    let blocks: Vec<&str> = re.split(text).map(|s| s.trim()).collect();
    // blocks[0] 恒空（"草案1："在开头切出空头）——blocks[n] = 草案 n
    if blocks.len() > wd {
        return blocks[wd].to_string();
    }
    if blocks.len() > 1 {
        return blocks[1].to_string();
    }
    text.to_string()
}

/// 检测意见中是否含对某草案的满意表态（Web has_satisfied 移植）
pub fn has_satisfied(opinion: &str, draft_num: u32) -> bool {
    opinion.contains(&format!("草案{draft_num}：满意"))
        || opinion.contains(&format!("草案{draft_num}:满意"))
        || opinion.contains(&format!("**草案{draft_num}**：满意"))
}

/// 按编号提取草案段落（Web extract_draft_segment 移植）
pub fn extract_draft_segment(drafts: &str, sel_num: u32) -> String {
    let pat = regex::Regex::new(r"草案(\d)[：:]\s*").unwrap();
    let marks: Vec<(usize, usize)> = pat.find_iter(drafts).map(|m| (m.start(), m.end())).collect();
    if marks.is_empty() {
        return if sel_num == 1 {
            drafts.trim().to_string()
        } else {
            String::new()
        };
    }
    if sel_num == 1 {
        return drafts[..marks[0].0].trim().to_string();
    }
    let sel = sel_num as usize;
    if sel <= marks.len() + 1 {
        let start = marks[sel - 2].1;
        let end = if sel - 1 < marks.len() {
            marks[sel - 1].0
        } else {
            drafts.len()
        };
        return drafts[start..end].trim().to_string();
    }
    String::new()
}

/// 草案段与主持人总结的 2 字词重叠（Web draft_bigram_overlap 移植——过滤虚词）
pub fn draft_bigram_overlap(seg: &str, summary: &str) -> (usize, Vec<String>) {
    if seg.is_empty() || summary.is_empty() {
        return (0, vec![]);
    }
    fn bigrams(text: &str) -> std::collections::HashSet<String> {
        let cleaned: String = text
            .chars()
            .filter(|c| !" \t\n\r，。；：、！？\"“”'‘’（）()《》[]…—-".contains(*c))
            .collect();
        let mut out = std::collections::HashSet::new();
        let chars: Vec<char> = cleaned.chars().collect();
        for i in 0..chars.len().saturating_sub(1) {
            let b: String = chars[i..i + 2].iter().collect();
            if b.chars().any(|c| "的了是在与和或".contains(c)) {
                continue;
            }
            out.insert(b);
        }
        out
    }
    let seg_set = bigrams(seg);
    let sum_set = bigrams(summary);
    let mut hits: Vec<String> = seg_set.intersection(&sum_set).cloned().collect();
    hits.sort();
    let sample = hits.iter().take(6).cloned().collect();
    (hits.len(), sample)
}

/// 格式化字段值（Web format_field_value）
pub fn format_field_value(v: &str) -> String {
    if v.is_empty() || v == "_" {
        String::new()
    } else {
        v.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fields_text() {
        let m = parse_fields("## 时代背景:仙侠\n## 地理格局：九州\n地理=东胜");
        assert_eq!(m.get("时代背景").map(|s| s.as_str()), Some("仙侠"));
        assert_eq!(m.get("地理格局").map(|s| s.as_str()), Some("九州"));
        assert_eq!(m.get("地理").map(|s| s.as_str()), Some("东胜"));
    }

    #[test]
    fn parse_fields_json() {
        let m = parse_fields(r#"{"时代背景": "灵气复苏", "格局": null}"#);
        assert_eq!(m.get("时代背景").map(|s| s.as_str()), Some("灵气复苏"));
        assert_eq!(m.len(), 1, "null 跳过");
    }

    #[test]
    fn pick_draft_variants() {
        let t = "草案1：第一版\n草案2：第二版\n草案3：第三版";
        // blocks[0]=空头——wd=1 → 草案1 内容（Web 版同语义）
        assert_eq!(pick_draft(t, 1), "第一版");
        assert_eq!(pick_draft(t, 2), "第二版");
    }

    #[test]
    fn satisfied_detection() {
        assert!(has_satisfied("草案2：满意，理由...", 2));
        assert!(!has_satisfied("草案1：不满意", 2));
        assert!(has_satisfied("**草案3**：满意", 3));
    }

    #[test]
    fn extract_segments() {
        let t = "这是草案1内容\n草案2：第二版内容\n草案3：第三版内容";
        assert!(extract_draft_segment(t, 1).contains("这是草案1内容"));
        assert_eq!(extract_draft_segment(t, 2), "第二版内容");
        assert_eq!(extract_draft_segment(t, 3), "第三版内容");
    }

    #[test]
    fn bigram_overlap_detects() {
        let seg = "九州大陆灵气复苏，宗门林立";
        let sum = "作者同意九州大陆灵气复苏的设定";
        let (n, _) = draft_bigram_overlap(seg, sum);
        assert!(n >= 3, "应有重叠——实际 {n}");
    }
}
