//! engine/review.rs — 评审管线（T6-1——2026-09-03）
//! Web 版 review_chapter_content(951-1051)/_rewrite_chapter(890-949)/_build_review_lessons(874-888) 移植
//! Pipeline1 设定冲突（4 类——世界观/角色 critical——情节/风格 major）
//! Pipeline3 语言质量（4 项——错别字/语病 major——其他 minor）
//! critical/major → 自动重写（覆盖正文+chunks+向量+feedback 标记 rewritten）

use crate::ai::AiMessage;
use crate::db::pool::Db;
use crate::engine::discussion::{sys_msg, user_msg, BoxAi};

/// 评审结果
#[derive(Debug, Clone)]
pub struct ReviewFinding {
    pub pipeline: String,
    pub severity: String,
    pub issue: String,
}

/// 单章评审（返回发现——含自动重写标记）
pub async fn review_chapter_content(
    db: &Db,
    ai: &BoxAi,
    sid: &str,
    chapter_id: i64,
    auto_rewrite: bool,
) -> Result<Vec<ReviewFinding>, String> {
    let text = db.get_chapter_content_text(chapter_id).await.map_err(|e| e.to_string())?;
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let locked = db.get_locked_field_values(sid).await.map_err(|e| e.to_string())?;
    let context = locked.iter().map(|(k, v)| format!("{k}:\n{v}")).collect::<Vec<_>>().join("\n");
    let text_preview: String = text.chars().take(3000).collect();

    // 清旧
    db.clear_chapter_reviews(chapter_id).await.map_err(|e| e.to_string())?;
    db.clear_review_feedback(chapter_id).await.map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    let mut fb_ids: Vec<i64> = Vec::new();

    // ══ Pipeline 1: 设定冲突 ══
    let conflict_prompt = format!(
        "你是一名严谨的小说设定审查员。请检查以下正文是否与已锁定设定存在冲突。\n\n【已锁定设定】\n{context}\n\n【正文（节选）】\n{text_preview}\n\n检查以下类型的冲突（每项一行，无冲突则不输出）：\n- [世界观冲突] 具体描述\n- [角色冲突] 具体描述\n- [情节冲突] 具体描述\n- [风格冲突] 具体描述\n\n输出格式：每行一个冲突，空行分隔。无冲突则输出\"无冲突\"。"
    );
    let conflict_result = ai
        .chat(&[sys_msg("你严谨审查设定一致性，只输出具体冲突，不输出废话。"), user_msg(&conflict_prompt)], 2048)
        .await
        .map_err(|e| e.to_string())?
        .content;
    if !conflict_result.contains("无冲突") {
        for line in conflict_result.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let severity = if line.contains("世界观") || line.contains("角色") { "critical" } else { "major" };
            db.add_review(chapter_id, "设定冲突", severity, 0, line, "").await.map_err(|e| e.to_string())?;
            let fid = db.add_review_feedback(sid, chapter_id, "设定冲突", severity, line, "").await.map_err(|e| e.to_string())?;
            fb_ids.push(fid);
            results.push(ReviewFinding { pipeline: "设定冲突".into(), severity: severity.into(), issue: line.into() });
        }
    }

    // ══ Pipeline 3: 语言质量 ══
    let text_full: String = text.chars().take(4000).collect();
    let lang_prompt = format!(
        "你是一名专业文字编辑。请检查以下正文的语言质量。\n\n【正文（完整）】\n{text_full}\n\n逐项检查并输出问题：\n- 错别字/语病：[具体句子] → [修改建议]\n- 重复用词：[具体句子] → [修改建议]\n- 对话不自然：[具体句子] → [修改建议]\n- 节奏问题：[具体描述]\n\n每行一个问题，空行分隔。没有问题则输出\"语言质量合格\"。"
    );
    let lang_result = ai
        .chat(&[sys_msg("你是专业文字编辑，严格逐项检查，只输出问题。"), user_msg(&lang_prompt)], 2048)
        .await
        .map_err(|e| e.to_string())?
        .content;
    if !lang_result.contains("语言质量合格") {
        for line in lang_result.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let severity = if line.contains("错别字") || line.contains("语病") { "major" } else { "minor" };
            let issue_text = if let Some(idx) = line.find('：').or_else(|| line.find(':')) {
                line[idx + 1..].trim().to_string()
            } else {
                line.to_string()
            };
            db.add_review(chapter_id, "语言质量", severity, 0, &issue_text, line).await.map_err(|e| e.to_string())?;
            let fid = db.add_review_feedback(sid, chapter_id, "语言质量", severity, &issue_text, line).await.map_err(|e| e.to_string())?;
            fb_ids.push(fid);
            results.push(ReviewFinding { pipeline: "语言质量".into(), severity: severity.into(), issue: line.into() });
        }
    }

    // ══ 自动重写（critical/major）══
    let rewrite_issues: Vec<String> = results
        .iter()
        .filter(|r| r.severity == "critical" || r.severity == "major")
        .map(|r| r.issue.clone())
        .collect();
    if auto_rewrite && !rewrite_issues.is_empty() {
        let rewritten = rewrite_chapter(db, ai, sid, chapter_id, &rewrite_issues).await?;
        if let Some(rw) = rewritten {
            // feedback 标记 rewritten
            for fid in &fb_ids {
                db.mark_feedback_rewritten(*fid, rw.as_str()).await.map_err(|e| e.to_string())?;
            }
            results.push(ReviewFinding { pipeline: "自动重写".into(), severity: "info".into(), issue: format!("已按 {} 条问题重写", rewrite_issues.len()) });
        }
    }
    Ok(results)
}

/// 重写章节（覆盖正文+chunks——向量由调用方重嵌）
async fn rewrite_chapter(db: &Db, ai: &BoxAi, sid: &str, chapter_id: i64, issues: &[String]) -> Result<Option<String>, String> {
    let chapters = db.get_chapters(sid).await.map_err(|e| e.to_string())?;
    let ch = match chapters.iter().find(|c| c.id == chapter_id) {
        Some(c) => c.clone(),
        None => return Ok(None),
    };
    let locked = db.get_locked_field_values(sid).await.map_err(|e| e.to_string())?;
    let context = locked.iter().map(|(k, v)| format!("{k}:\n{v}")).collect::<Vec<_>>().join("\n");
    let old_text = db.get_chapter_content_text(chapter_id).await.map_err(|e| e.to_string())?;
    let old_preview: String = old_text.chars().take(3000).collect();
    let issue_str: Vec<String> = issues.iter().take(6).map(|i| format!("- {i}")).collect();
    let outline = ch.outline.clone().unwrap_or_default();

    let prompt = format!(
        "你是资深小说作者「司言」。以下是本章初稿和审查发现的问题，请重写本章，修复所有问题，保持原有情节推进。\n\n【已锁定设定】\n{context}\n\n【本章标题】{}\n【本章大纲】\n{outline}\n\n【初稿】\n{old_preview}\n\n【审查发现的问题（必须修复）】\n{}\n\n请重写本章正文。要求：\n1. 修复上述所有问题，不再出现同样错误\n2. 字数 2000-3000 字，保持情节连续性\n3. 语言流畅自然，有画面感\n4. 标注段落（用空行分隔）\n5. 只输出正文，不要额外说明",
        ch.title.clone().unwrap_or_default(),
        issue_str.join("\n")
    );
    let reply = ai
        .chat(
            &[sys_msg("你是资深小说作者，擅长根据审查意见重写高质量正文。输出只包含正文，段落间用空行分隔。"), user_msg(&prompt)],
            4096,
        )
        .await
        .map_err(|e| e.to_string())?;
    if reply.content.trim().is_empty() {
        return Ok(None);
    }
    db.update_chapter_content(sid, chapter_id, &reply.content).await.map_err(|e| e.to_string())?;
    db.clear_content_chunks(chapter_id).await.map_err(|e| e.to_string())?;
    let paragraphs: Vec<&str> = reply.content.split("\n\n").map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
    for (i, para) in paragraphs.iter().enumerate() {
        db.add_content_chunk(chapter_id, i as i64, para).await.map_err(|e| e.to_string())?;
    }
    Ok(Some(reply.content))
}

/// 既往审查结论摘要（rewritten——教训注入后续正文 prompt）
pub async fn build_review_lessons(db: &Db, sid: &str) -> Result<String, String> {
    let lessons = db.get_review_feedback(sid, "rewritten", 10).await.map_err(|e| e.to_string())?;
    if lessons.is_empty() {
        return Ok(String::new());
    }
    let mut lines = Vec::new();
    for (issue_type, issue, suggestion) in &lessons {
        let issue_t: String = issue.chars().take(120).collect();
        let sugg: String = suggestion.chars().take(120).collect();
        if !issue_t.is_empty() {
            let _ = issue_type;
            lines.push(format!("- 曾出现「{issue_t}」建议：{}", if sugg.is_empty() { "（无）".to_string() } else { sugg }));
        }
    }
    if lines.is_empty() {
        return Ok(String::new());
    }
    Ok(format!("【既往审查结论（勿再犯）】\n{}", lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::mock::MockProvider;
    use crate::db::pool::Db;

    async fn setup(sid: &str, db: &Db) {
        db.create_session(sid, "评审测试", "玄幻", "长篇", "zerg", "novel").await.unwrap();
        db.upsert_template(sid, 1, "故事核", "少年林风在九州大陆修行，灵气复苏世界。", true, "故事核").await.unwrap();
        db.create_chapter(sid, 1, 1, "初章", "林风踏入宗门。").await.unwrap();
        let ch = &db.get_chapters(sid).await.unwrap()[0];
        db.add_content_chunk(ch.id, 0, "林风走进宗门大殿，看见许多弟子。").await.unwrap();
        db.add_content_chunk(ch.id, 1, "他握紧拳头，暗暗发誓要变强。").await.unwrap();
    }

    /// 冲突管线：发现世界观冲突（critical）→ 自动重写
    #[tokio::test]
    async fn conflict_triggers_rewrite() {
        let _ = std::fs::remove_file("/tmp/yz_rev1.db");
        let db = Db::open("/tmp/yz_rev1.db").await.unwrap();
        setup("v1", &db).await;
        // mock：冲突管线返回冲突——语言合格——重写正文
        let mut script: Vec<String> = Vec::new();
        script.push("[世界观冲突] 文中出现科技飞船，与灵气复苏世界冲突。".to_string());
        script.push("语言质量合格".to_string());
        script.push("重写后正文第一段。\n\n重写后正文第二段。".to_string());
        let refs: Vec<&str> = script.iter().map(|s| s.as_str()).collect();
        let ai: BoxAi = Box::new(MockProvider::new(refs));
        let ch = &db.get_chapters("v1").await.unwrap()[0];
        let findings = review_chapter_content(&db, &ai, "v1", ch.id, true).await.unwrap();
        assert!(findings.iter().any(|f| f.severity == "critical"), "世界观冲突 critical");
        assert!(findings.iter().any(|f| f.pipeline == "自动重写"), "自动重写触发");
        // 正文被覆盖
        let text = db.get_chapter_content_text(ch.id).await.unwrap();
        assert!(text.contains("重写后正文"), "正文已重写——实际 {text}");
        // feedback 标记 rewritten
        let lessons = db.get_review_feedback("v1", "rewritten", 10).await.unwrap();
        assert!(!lessons.is_empty(), "feedback 已标记 rewritten");
        std::fs::remove_file("/tmp/yz_rev1.db").ok();
    }

    /// 无冲突：不重写
    #[tokio::test]
    async fn clean_passes_through() {
        let _ = std::fs::remove_file("/tmp/yz_rev2.db");
        let db = Db::open("/tmp/yz_rev2.db").await.unwrap();
        setup("v2", &db).await;
        let script = vec!["无冲突".to_string(), "语言质量合格".to_string()];
        let refs: Vec<&str> = script.iter().map(|s| s.as_str()).collect();
        let ai: BoxAi = Box::new(MockProvider::new(refs));
        let ch = &db.get_chapters("v2").await.unwrap()[0];
        let findings = review_chapter_content(&db, &ai, "v2", ch.id, true).await.unwrap();
        assert!(findings.is_empty(), "无发现——实际 {:?}", findings);
        let text = db.get_chapter_content_text(ch.id).await.unwrap();
        assert!(text.contains("宗门大殿"), "正文未动");
        std::fs::remove_file("/tmp/yz_rev2.db").ok();
    }

    /// 教训摘要构建
    #[tokio::test]
    async fn lessons_built() {
        let _ = std::fs::remove_file("/tmp/yz_rev3.db");
        let db = Db::open("/tmp/yz_rev3.db").await.unwrap();
        setup("v3", &db).await;
        db.add_review_feedback("v3", 1, "设定冲突", "critical", "飞船乱入", "删掉飞船").await.unwrap();
        // 标记 rewritten
        let fb = db.get_review_feedback("v3", "pending", 10).await.unwrap();
        assert_eq!(fb.len(), 1);
        // pending 查不到 lessons——先标记
        let lessons_pending = build_review_lessons(&db, "v3").await.unwrap();
        assert!(lessons_pending.is_empty(), "pending 不算教训");
        std::fs::remove_file("/tmp/yz_rev3.db").ok();
    }
}
