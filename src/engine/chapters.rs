//! engine/chapters.rs — 章节生成链（T5-1——2026-09-03）
//! Web 版 generate_chapters(639-708)/generate_chapter_content(796-872) 移植
//! 干净版编号（Web 有 next 预分配错位 bug——Rust 版自增连续——不移植 bug）
//! 正文链：设定+大纲+记忆(向量 T5-2 接入点)→正文提示词→段落 chunks 落库

use crate::ai::AiMessage;
use crate::db::pool::Db;
use crate::engine::discussion::{sys_msg, user_msg, BoxAi};

/// 生成章节大纲（AI——行解析"第X章:标题"+后续行积累大纲——自增编号）
pub async fn generate_chapters(
    db: &Db,
    ai: &BoxAi,
    sid: &str,
    num_chapters: i64,
    chapters_per_volume: i64,
) -> Result<usize, String> {
    let session = db
        .get_session(sid)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("会话不存在 {sid}"))?;
    let locked = db
        .get_locked_field_values(sid)
        .await
        .map_err(|e| e.to_string())?;
    if locked.is_empty() {
        return Err("没有已锁定设定——先跑讨论".into());
    }
    let context = locked
        .iter()
        .map(|(k, v)| format!("{k}:\n{v}"))
        .collect::<Vec<_>>()
        .join("\n");

    let chapter_prompt = format!(
        "你是圆桌派的主持人。已锁定内容：\n{context}\n\n请生成 {num_chapters} 章的详细大纲。格式要求：\n每章包含：\n- 章节标题\n- 大纲内容（200-500字）\n- 关键角色\n- 情绪基调\n\n按卷组织章节，每卷 {chapters_per_volume} 章。"
    );
    let reply = ai
        .chat(
            &[
                sys_msg("你是圆桌派的主持人，擅长根据设定生成章节大纲。"),
                user_msg(&chapter_prompt),
            ],
            0,
        )
        .await
        .map_err(|e| e.to_string())?;

    // 行解析（干净自增——Web next 预分配 bug 修复）
    let mut created = 0usize;
    let mut volume: i64 = 1;
    let mut chapter_num: i64 = 0;
    let mut title = String::new();
    let mut outline_lines: Vec<String> = Vec::new();
    let mut seen_header = false;

    for raw in reply.content.lines() {
        let line = raw.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let is_header = (line.starts_with("第") && line.contains("章") && line.contains(':'))
            || (line.starts_with("第") && line.contains("章") && line.contains('：'));
        if is_header {
            // 存前一章
            if seen_header && !title.is_empty() {
                db.create_chapter(sid, volume, chapter_num, &title, &outline_lines.join("\n"))
                    .await
                    .map_err(|e| e.to_string())?;
                created += 1;
            }
            let sep = if line.contains(':') { ':' } else { '：' };
            title = line
                .split(sep)
                .nth(1)
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            // 卷翻页
            chapter_num += 1;
            if chapter_num > chapters_per_volume {
                chapter_num = 1;
                volume += 1;
            }
            outline_lines.clear();
            seen_header = true;
        } else if seen_header {
            outline_lines.push(line);
        }
    }
    // 最后一章
    if seen_header && !title.is_empty() {
        db.create_chapter(sid, volume, chapter_num, &title, &outline_lines.join("\n"))
            .await
            .map_err(|e| e.to_string())?;
        created += 1;
    }
    Ok(created)
}

/// 生成单章正文（设定+大纲→正文——段落 chunks 落库——向量/评审由调用方接）
pub async fn generate_chapter_content(
    db: &Db,
    ai: &BoxAi,
    sid: &str,
    volume: i64,
    chapter_number: i64,
) -> Result<i64, String> {
    let locked = db
        .get_locked_field_values(sid)
        .await
        .map_err(|e| e.to_string())?;
    let chapters = db.get_chapters(sid).await.map_err(|e| e.to_string())?;
    let ch = chapters
        .iter()
        .find(|c| c.volume == volume && c.chapter_number == chapter_number)
        .cloned()
        .ok_or_else(|| format!("Chapter v{volume} ch{chapter_number} not found"))?;
    let context = locked
        .iter()
        .map(|(k, v)| format!("{k}:\n{v}"))
        .collect::<Vec<_>>()
        .join("\n");
    let outline = ch.outline.clone().unwrap_or_default();

    let prompt = format!(
        "你是资深小说作者「司言」，专精语言质感与描写。\n\n【已锁定设定】\n{context}\n\n【本章大纲】\n{outline}\n\n【本章标题】\n{}\n\n请根据以上设定和大纲，写出本章正文。要求：\n1. 字数 2000-3000 字\n2. 与已锁定设定完全一致（世界观/角色/风格），且与角色当前状态保持一致\n3. 语言流畅自然，有画面感\n4. 标注段落（用空行分隔）\n5. 只输出正文，不要额外说明",
        ch.title.clone().unwrap_or_default()
    );
    let reply = ai
        .chat(
            &[
                sys_msg("你是资深小说作者，擅长根据设定和大纲创作高质量正文。输出只包含正文，段落间用空行分隔。"),
                user_msg(&prompt),
            ],
            4096,
        )
        .await
        .map_err(|e| e.to_string())?;
    if reply.content.trim().is_empty() {
        return Err("正文生成返回空".into());
    }
    // 落库正文 + 段落 chunks
    db.update_chapter_content(sid, ch.id, &reply.content)
        .await
        .map_err(|e| e.to_string())?;
    db.clear_content_chunks(ch.id)
        .await
        .map_err(|e| e.to_string())?;
    let paragraphs: Vec<&str> = reply
        .content
        .split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    for (i, para) in paragraphs.iter().enumerate() {
        db.add_content_chunk(ch.id, i as i64, para)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(ch.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::mock::MockProvider;
    use crate::db::pool::Db;

    async fn setup(sid: &str, db: &Db) {
        db.create_session(sid, "章节测试", "玄幻", "长篇", "zerg", "novel")
            .await
            .unwrap();
        db.upsert_template(sid, 1, "故事核", "少年穿越仙侠大陆修行。", true, "故事核")
            .await
            .unwrap();
        db.upsert_template(sid, 2, "世界观", "九州大陆，灵气复苏。", true, "世界观")
            .await
            .unwrap();
    }

    /// 大纲解析：5 章 2 卷（卷 1: ch1-2, 卷 2: ch3）
    #[tokio::test]
    async fn outline_parses_volumes() {
        let _ = std::fs::remove_file("/tmp/yz_ch1.db");
        let db = Db::open("/tmp/yz_ch1.db").await.unwrap();
        setup("c1", &db).await;
        let outline = "第1章:少年启程\n第一章开头，少年离开山村。\n\n第2章:初入宗门\n少年到达宗门，接受考验。\n\n第3章:秘境试炼\n卷二开启，少年进入秘境。";
        let refs: Vec<&str> = vec![outline];
        let ai: BoxAi = Box::new(MockProvider::new(refs));
        let n = generate_chapters(&db, &ai, "c1", 3, 2).await.unwrap();
        assert_eq!(n, 3, "3 章生成");
        let chs = db.get_chapters("c1").await.unwrap();
        assert_eq!(chs.len(), 3);
        assert_eq!(chs[0].chapter_number, 1);
        assert_eq!(chs[0].title.as_deref(), Some("少年启程"));
        assert!(
            chs[0].outline.clone().unwrap_or_default().contains("山村"),
            "大纲积累"
        );
        assert_eq!(chs[1].chapter_number, 2);
        assert_eq!(chs[2].volume, 2, "第 3 章进卷 2");
        assert_eq!(chs[2].chapter_number, 1, "卷 2 第 1 章");
        std::fs::remove_file("/tmp/yz_ch1.db").ok();
    }

    /// 正文生成：段落 chunks 落库
    #[tokio::test]
    async fn content_writes_chunks() {
        let _ = std::fs::remove_file("/tmp/yz_ch2.db");
        let db = Db::open("/tmp/yz_ch2.db").await.unwrap();
        setup("c2", &db).await;
        // 预置 1 章
        db.create_chapter("c2", 1, 1, "少年启程", "少年离开山村，踏上修行路。")
            .await
            .unwrap();
        let body = "第一段：少年站在村口。\n\n第二段：他握紧拳头，走向远方。\n\n第三段：山路蜿蜒。";
        let refs: Vec<&str> = vec![body];
        let ai: BoxAi = Box::new(MockProvider::new(refs));
        let cid = generate_chapter_content(&db, &ai, "c2", 1, 1)
            .await
            .unwrap();
        assert!(cid > 0);
        let chunks = db.get_content_chunks(cid).await.unwrap();
        assert_eq!(chunks.len(), 3, "3 段落");
        assert_eq!(chunks[0].paragraph_index, 0);
        assert!(chunks[0].text.contains("村口"));
        // 章 content 更新
        let chs = db.get_chapters("c2").await.unwrap();
        assert!(chs[0]
            .content
            .clone()
            .unwrap_or_default()
            .contains("山路蜿蜒"));
        std::fs::remove_file("/tmp/yz_ch2.db").ok();
    }
}
