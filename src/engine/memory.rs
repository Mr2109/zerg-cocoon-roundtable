//! engine/memory.rs — 章节记忆链（T5-2——2026-09-03）
//! Web 版 _build_memory_context/_update_character_states/_embed_chapter/_retrieve_relevant 移植
//! 向量：embed::embedder（bge-small-zh 512 维——归一化余弦）

use crate::db::pool::Db;
use crate::embed::embedder;

/// 构建记忆上下文（设定库 30 条 + 角色状态 20 条——注入正文 prompt）
pub async fn build_memory_context(db: &Db, sid: &str) -> Result<String, String> {
    let mut parts = Vec::new();
    let settings = db
        .get_world_settings(sid, None, 30)
        .await
        .map_err(|e| e.to_string())?;
    if !settings.is_empty() {
        let lines: Vec<String> = settings
            .iter()
            .filter_map(|s| {
                let key = &s.key;
                let val = s.value.clone().unwrap_or_default();
                if !key.is_empty() && !val.is_empty() {
                    Some(format!("{key}: {}", truncate(&val, 500)))
                } else {
                    None
                }
            })
            .collect();
        if !lines.is_empty() {
            parts.push(format!("【已锁定设定库】\n{}", lines.join("\n")));
        }
    }
    let chars = db
        .get_character_states(sid, 20)
        .await
        .map_err(|e| e.to_string())?;
    if !chars.is_empty() {
        let clines: Vec<String> = chars
            .iter()
            .filter_map(|c| {
                let st = c.state_json.clone().unwrap_or_default();
                if !c.character_name.is_empty() && !st.is_empty() {
                    Some(format!("- {}: {}", c.character_name, truncate(&st, 300)))
                } else {
                    None
                }
            })
            .collect();
        if !clines.is_empty() {
            parts.push(format!("【角色当前状态】\n{}", clines.join("\n")));
        }
    }
    Ok(parts.join("\n\n"))
}

/// 从正文更新角色状态（Web 启发式：设定库已知名 + 正文高频段——兜底主角 5）
pub async fn update_character_states(
    db: &Db,
    sid: &str,
    text: &str,
    chapter_number: i64,
) -> Result<usize, String> {
    let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();
    let chars_setting = db
        .get_world_settings(sid, Some("character"), 50)
        .await
        .map_err(|e| e.to_string())?;
    for s in &chars_setting {
        let val = s.value.clone().unwrap_or_default();
        for seg in split_segments(&val) {
            let c = seg.chars().count();
            if c >= 2 && c <= 8 {
                known.insert(seg);
            }
        }
    }
    let mut counter: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    // Web 滑窗语义：连续段内每位置 2/3/4 字候选
    for seg in split_segments(text) {
        let chars: Vec<char> = seg.chars().collect();
        for w in 2..=4 {
            if chars.len() < w {
                continue;
            }
            for i in 0..=chars.len() - w {
                let s: String = chars[i..i + w].iter().collect();
                *counter.entry(s).or_insert(0) += 1;
            }
        }
    }
    let mut active: Vec<String> = counter
        .iter()
        .filter(|(n, cnt)| **cnt >= 3 && (known.is_empty() || known.contains(*n)))
        .map(|(n, _)| n.clone())
        .collect();
    active.sort_by_key(|n| std::cmp::Reverse(counter.get(n).copied().unwrap_or(0)));
    active.truncate(30);
    if active.is_empty() && !known.is_empty() {
        active = known.iter().take(5).cloned().collect();
    }
    for name in &active {
        db.upsert_character_state(sid, name, "最近章节出现", chapter_number)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(active.len())
}

/// 章节正文向量化入库（段落级——返回段数）
pub async fn embed_chapter(
    db: &Db,
    sid: &str,
    chapter_id: i64,
    text: &str,
) -> Result<usize, String> {
    let paragraphs: Vec<String> = text
        .split("\n\n")
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    if paragraphs.is_empty() {
        return Ok(0);
    }
    db.clear_chapter_embeddings(chapter_id)
        .await
        .map_err(|e| e.to_string())?;
    let vecs = embedder::embed(&paragraphs)?;
    for (i, (para, vec)) in paragraphs.iter().zip(vecs.iter()).enumerate() {
        db.save_chunk_embedding(sid, chapter_id, i as i64, vec, para)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(vecs.len())
}

/// 语义检索（query 嵌入 → 全库余弦 top-k > 0.35——排除章节可选）
pub async fn retrieve_relevant(
    db: &Db,
    sid: &str,
    query: &str,
    exclude_chapter_id: Option<i64>,
    k: usize,
) -> Result<Vec<String>, String> {
    let all = db
        .get_all_embeddings(sid, exclude_chapter_id)
        .await
        .map_err(|e| e.to_string())?;
    if all.is_empty() {
        return Ok(Vec::new());
    }
    let qs: String = query.chars().take(800).collect();
    let qv = match embedder::embed(&[qs]) {
        Ok(mut v) => v.drain(..).next().unwrap_or_default(),
        Err(e) => return Err(e),
    };
    let mut scored: Vec<(f32, String)> = all
        .into_iter()
        .filter(|(ev, _)| !ev.is_empty())
        .map(|(ev, text)| (embedder::cosine(&qv, ev.as_slice()), text))
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    Ok(scored
        .into_iter()
        .take(k)
        .filter(|(s, _)| *s > 0.35)
        .map(|(_, t)| t)
        .collect())
}

/// 按标点/空白切出连续文本段（中文角色名候选——避免 regex 字符类转义）
fn split_segments(text: &str) -> Vec<String> {
    let mut segs = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            cur.push(ch);
        } else if !cur.is_empty() {
            segs.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        segs.push(cur);
    }
    segs
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::pool::Db;

    async fn setup(sid: &str, db: &Db) {
        db.create_session(sid, "记忆测试", "玄幻", "长篇", "zerg", "novel")
            .await
            .unwrap();
        db.upsert_world_setting(sid, "character", "主角", "少年林风 穿越者")
            .await
            .unwrap();
        db.upsert_world_setting(sid, "world", "大陆", "九州大陆灵气复苏")
            .await
            .unwrap();
        db.upsert_character_state(sid, "林风", "境界筑基初期", 1)
            .await
            .unwrap();
    }

    /// 记忆上下文含设定库+角色状态
    #[tokio::test]
    async fn memory_context_built() {
        let _ = std::fs::remove_file("/tmp/yz_mem1.db");
        let db = Db::open("/tmp/yz_mem1.db").await.unwrap();
        setup("m1", &db).await;
        let ctx = build_memory_context(&db, "m1").await.unwrap();
        assert!(ctx.contains("设定库"), "设定库段");
        assert!(ctx.contains("林风"), "角色状态");
        std::fs::remove_file("/tmp/yz_mem1.db").ok();
    }

    /// 角色状态更新（正文高频名）
    #[tokio::test]
    async fn character_update_from_text() {
        let _ = std::fs::remove_file("/tmp/yz_mem2.db");
        let db = Db::open("/tmp/yz_mem2.db").await.unwrap();
        setup("m2", &db).await;
        let text = "林风握紧剑柄，林风抬头看天，林风迈出一步，林风低声道。";
        let n = update_character_states(&db, "m2", text, 2).await.unwrap();
        assert!(n >= 1, "至少更新 1 角色");
        let states = db.get_character_states("m2", 20).await.unwrap();
        assert!(
            states
                .iter()
                .any(|c| c.character_name.contains("林风") && c.last_seen_chapter >= 2),
            "主角状态已更新——实际 {:?}",
            states.iter().map(|c| &c.character_name).collect::<Vec<_>>()
        );
        std::fs::remove_file("/tmp/yz_mem2.db").ok();
    }

    /// 向量存/取 + 语义检索（真嵌入——模型缓存已就绪）
    #[tokio::test]
    async fn embed_and_retrieve() {
        match crate::embed::embedder::init() {
            Ok(()) => {}
            Err(e) => {
                log::warn!("fastembed 不可用: {e}——跳过");
                return;
            }
        }
        let _ = std::fs::remove_file("/tmp/yz_mem3.db");
        let db = Db::open("/tmp/yz_mem3.db").await.unwrap();
        db.create_session("m3", "记忆检索", "玄幻", "长篇", "zerg", "novel")
            .await
            .unwrap();
        db.create_chapter("m3", 1, 1, "测试章", "大纲")
            .await
            .unwrap();
        let ch = &db.get_chapters("m3").await.unwrap()[0];
        let body1 = "少年林风踏入秘境，灵气浓郁，妖兽低吼。";
        embed_chapter(&db, "m3", ch.id, body1).await.unwrap();
        let rel = retrieve_relevant(&db, "m3", "秘境里有什么危险", None, 3)
            .await
            .unwrap();
        assert!(!rel.is_empty(), "应检索到相关片段");
        let v = db.get_all_embeddings("m3", None).await.unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0.len(), 512, "512 维");
        crate::embed::embedder::reset();
        std::fs::remove_file("/tmp/yz_mem3.db").ok();
    }
}
