//! db/crud.rs — CRUD 方法集（T2-3——2026-09-03——移植 Web 版 database.py）
//! 模式：SQL 照抄 Web 版——参数同——返回类型化 models——行为对齐

use super::models::*;
use super::pool::{Db, DbResult};
use rusqlite::params;

/// 按列名取 sessions 全字段（SELECT * 列序对齐 SCHEMA_SQL）
const SESSION_COLS: &str = "id, name, novel_type, length, project_type, status, current_block, state_json, provider, total_tokens, created_at, updated_at";

impl Db {
    // ─── sessions ───
    pub async fn create_session(
        &self,
        id: &str,
        name: &str,
        novel_type: &str,
        length: &str,
        provider: &str,
        project_type: &str,
    ) -> DbResult<()> {
        let (id, name, novel_type, length, provider, project_type) = (
            id.to_string(),
            name.to_string(),
            novel_type.to_string(),
            length.to_string(),
            provider.to_string(),
            project_type.to_string(),
        );
        self.call(move |c| {
            c.execute(
                &format!(
                    "INSERT INTO sessions (id, name, novel_type, length, project_type, current_block, provider) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)"
                ),
                params![id, name, novel_type, length, project_type, provider],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn get_session(&self, id: &str) -> DbResult<Option<Session>> {
        let id = id.to_string();
        self.query(move |c| {
            let mut stmt = c.prepare(&format!("SELECT {SESSION_COLS} FROM sessions WHERE id = ?1"))?;
            let mut rows = stmt.query_map(params![id], Session::from_row)?;
            Ok(rows.next().transpose()?)
        })
        .await
    }

    pub async fn get_all_sessions(&self) -> DbResult<Vec<Session>> {
        self.query(move |c| {
            let mut stmt =
                c.prepare(&format!("SELECT {SESSION_COLS} FROM sessions ORDER BY created_at DESC"))?;
            let rows = stmt.query_map([], Session::from_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
    }

    pub async fn update_session_progress(&self, id: &str, current_block: i64, status: &str) -> DbResult<()> {
        let id = id.to_string();
        let status = status.to_string();
        self.call(move |c| {
            c.execute(
                "UPDATE sessions SET current_block = ?1, status = ?2, updated_at = datetime('now') WHERE id = ?3",
                params![current_block, status, id],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn update_session_tokens(&self, id: &str, prompt: i64, completion: i64) -> DbResult<()> {
        let id = id.to_string();
        self.call(move |c| {
            c.execute(
                "UPDATE sessions SET total_tokens = total_tokens + ?1 + ?2, updated_at = datetime('now') WHERE id = ?3",
                params![prompt, completion, id],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn update_session_state(&self, id: &str, state_json: &str) -> DbResult<()> {
        let id = id.to_string();
        let state = state_json.to_string();
        self.call(move |c| {
            c.execute(
                "UPDATE sessions SET state_json = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![state, id],
            )?;
            Ok(())
        })
        .await
    }

    // ─── messages ───
    pub async fn add_message(
        &self,
        sid: &str,
        sender: &str,
        content: &str,
        sender_type: &str,
        metadata: &str,
    ) -> DbResult<i64> {
        let (sid, sender, content, st, md) = (
            sid.to_string(),
            sender.to_string(),
            content.to_string(),
            sender_type.to_string(),
            metadata.to_string(),
        );
        self.call(move |c| {
            c.execute(
                "INSERT INTO messages (session_id, sender, content, sender_type, metadata) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![sid, sender, content, st, md],
            )?;
            Ok(c.last_insert_rowid())
        })
        .await
    }

    pub async fn get_messages(&self, sid: &str, after_id: i64) -> DbResult<Vec<Message>> {
        let sid = sid.to_string();
        self.query(move |c| {
            let mut stmt = c.prepare(
                "SELECT id, session_id, sender, content, sender_type, metadata, created_at FROM messages WHERE session_id = ?1 AND id > ?2 ORDER BY id ASC",
            )?;
            let rows = stmt.query_map(params![sid, after_id], Message::from_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
    }

    pub async fn get_all_messages(&self, sid: &str) -> DbResult<Vec<Message>> {
        let sid = sid.to_string();
        self.query(move |c| {
            let mut stmt = c.prepare(
                "SELECT id, session_id, sender, content, sender_type, metadata, created_at FROM messages WHERE session_id = ?1 ORDER BY id ASC",
            )?;
            let rows = stmt.query_map(params![sid], Message::from_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
    }

    // ─── templates（Block 字段值——锁定）───
    pub async fn upsert_template(
        &self,
        sid: &str,
        block_index: i64,
        field_name: &str,
        field_value: &str,
        locked: bool,
        block_name: &str,
    ) -> DbResult<()> {
        let (sid, fn_, fv, bn) = (
            sid.to_string(),
            field_name.to_string(),
            field_value.to_string(),
            block_name.to_string(),
        );
        let lk = locked as i64;
        self.call(move |c| {
            c.execute(
                "INSERT INTO templates (session_id, block_index, block_name, field_name, field_value, locked)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(session_id, block_index, field_name)
                 DO UPDATE SET field_value = ?5, locked = ?6, block_name = ?3",
                params![sid, block_index, bn, fn_, fv, lk],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn get_templates(&self, sid: &str, block_index: Option<i64>) -> DbResult<Vec<Template>> {
        let sid = sid.to_string();
        self.query(move |c| {
            let (sql, rows) = match block_index {
                Some(bi) => {
                    let mut stmt = c.prepare(
                        "SELECT id, session_id, block_index, block_name, field_name, field_value, locked, locked_at, score, score_detail, created_at FROM templates WHERE session_id = ?1 AND block_index = ?2 ORDER BY field_name",
                    )?;
                    let rows = stmt
                        .query_map(params![sid, bi], Template::from_row)?
                        .collect::<Result<Vec<_>, _>>()?;
                    (String::new(), rows)
                }
                None => {
                    let mut stmt = c.prepare(
                        "SELECT id, session_id, block_index, block_name, field_name, field_value, locked, locked_at, score, score_detail, created_at FROM templates WHERE session_id = ?1 ORDER BY block_index, field_name",
                    )?;
                    let rows = stmt
                        .query_map(params![sid], Template::from_row)?
                        .collect::<Result<Vec<_>, _>>()?;
                    (String::new(), rows)
                }
            };
            let _ = sql;
            Ok(rows)
        })
        .await
    }

    pub async fn get_locked_field_values(&self, sid: &str) -> DbResult<Vec<(String, String)>> {
        let sid = sid.to_string();
        self.query(move |c| {
            let mut stmt = c.prepare(
                "SELECT field_name, field_value FROM templates WHERE session_id = ?1 AND locked = 1",
            )?;
            let rows = stmt.query_map(params![sid], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
    }

    /// 锁定一个块的所有字段（只标 locked——不覆盖值——对齐 Web 版 lock_block）
    pub async fn lock_block(&self, sid: &str, block_name: &str) -> DbResult<()> {
        let (sid, bn) = (sid.to_string(), block_name.to_string());
        self.call(move |c| {
            c.execute(
                "UPDATE templates SET locked = 1, locked_at = datetime('now') WHERE session_id = ?1 AND block_name = ?2",
                params![sid, bn],
            )?;
            Ok(())
        })
        .await
    }

    // ─── world_settings ───
    pub async fn upsert_world_setting(&self, sid: &str, setting_type: &str, key: &str, value: &str) -> DbResult<()> {
        if key.is_empty() || value.is_empty() {
            return Ok(());
        }
        let (sid, st, k, v) = (
            sid.to_string(),
            setting_type.to_string(),
            key.to_string(),
            value.to_string(),
        );
        self.call(move |c| {
            c.execute(
                "INSERT INTO world_settings (session_id, setting_type, key, value)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(session_id, setting_type, key)
                 DO UPDATE SET value = excluded.value, updated_at = datetime('now')",
                params![sid, st, k, v],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn get_world_settings(&self, sid: &str, setting_type: Option<&str>, limit: i64) -> DbResult<Vec<WorldSetting>> {
        let sid = sid.to_string();
        let st = setting_type.map(|s| s.to_string());
        self.query(move |c| {
            let mut rows = Vec::new();
            if let Some(t) = &st {
                let mut stmt = c.prepare(
                    "SELECT id, session_id, setting_type, key, value, updated_at FROM world_settings WHERE session_id = ?1 AND setting_type = ?2 ORDER BY updated_at DESC LIMIT ?3",
                )?;
                rows = stmt
                    .query_map(params![sid, t, limit], WorldSetting::from_row)?
                    .collect::<Result<Vec<_>, _>>()?;
            } else {
                let mut stmt = c.prepare(
                    "SELECT id, session_id, setting_type, key, value, updated_at FROM world_settings WHERE session_id = ?1 ORDER BY updated_at DESC LIMIT ?2",
                )?;
                rows = stmt
                    .query_map(params![sid, limit], WorldSetting::from_row)?
                    .collect::<Result<Vec<_>, _>>()?;
            }
            Ok(rows)
        })
        .await
    }

    // ─── character_state ───
    pub async fn upsert_character_state(
        &self,
        sid: &str,
        character_name: &str,
        state_json: &str,
        last_seen_chapter: i64,
    ) -> DbResult<()> {
        if character_name.is_empty() {
            return Ok(());
        }
        let (sid, cn, sj) = (
            sid.to_string(),
            character_name.to_string(),
            state_json.to_string(),
        );
        self.call(move |c| {
            c.execute(
                "INSERT INTO character_state (session_id, character_name, state_json, last_seen_chapter)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(session_id, character_name)
                 DO UPDATE SET state_json = excluded.state_json, last_seen_chapter = excluded.last_seen_chapter, updated_at = datetime('now')",
                params![sid, cn, sj, last_seen_chapter],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn get_character_states(&self, sid: &str, limit: i64) -> DbResult<Vec<CharacterState>> {
        let sid = sid.to_string();
        self.query(move |c| {
            let mut stmt = c.prepare(
                "SELECT id, session_id, character_name, state_json, last_seen_chapter, updated_at FROM character_state WHERE session_id = ?1 ORDER BY updated_at DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![sid, limit], CharacterState::from_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
    }

    // ─── token_usage ───
    pub async fn add_token_usage(&self, sid: &str, block_name: &str, prompt: i64, completion: i64) -> DbResult<()> {
        let (sid, bn) = (sid.to_string(), block_name.to_string());
        self.call(move |c| {
            c.execute(
                "INSERT INTO token_usage (session_id, block_name, prompt_tokens, completion_tokens) VALUES (?1, ?2, ?3, ?4)",
                params![sid, bn, prompt, completion],
            )?;
            Ok(())
        })
        .await
    }

    /// 写入模板评分（Web database.py update_template_score 583 移植——质量门禁写分）
    pub async fn update_template_score(
        &self,
        sid: &str,
        block_index: i64,
        field_name: &str,
        score: i64,
        score_detail: &str,
    ) -> DbResult<()> {
        let sid = sid.to_string();
        let field_name = field_name.to_string();
        let score_detail = score_detail.to_string();
        self.query(move |c| {
            c.execute(
                "UPDATE templates SET score = ?1, score_detail = ?2 WHERE session_id = ?3 AND block_index = ?4 AND field_name = ?5",
                params![score, score_detail, sid, block_index, field_name],
            )?;
            Ok(())
        })
        .await
    }


    // ─── 章节（T5-1）───

    pub async fn create_chapter(&self, sid: &str, volume: i64, chapter_number: i64, title: &str, outline: &str) -> DbResult<()> {
        let sid = sid.to_string();
        let title = title.to_string();
        let outline = outline.to_string();
        self.query(move |c| {
            c.execute(
                "INSERT OR REPLACE INTO chapters (session_id, volume, chapter_number, title, outline) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![sid, volume, chapter_number, title, outline],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn update_chapter_content(&self, sid: &str, chapter_id: i64, content: &str) -> DbResult<()> {
        let sid = sid.to_string();
        let content = content.to_string();
        self.query(move |c| {
            c.execute(
                "UPDATE chapters SET content = ?1, updated_at = datetime('now') WHERE session_id = ?2 AND id = ?3",
                params![content, sid, chapter_id],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn get_chapters(&self, sid: &str) -> DbResult<Vec<crate::db::models::Chapter>> {
        let sid = sid.to_string();
        self.query(move |c| {
            let mut stmt = c.prepare(
                "SELECT id, session_id, volume, chapter_number, title, outline, content, locked, score, score_detail, created_at, updated_at FROM chapters WHERE session_id = ?1 ORDER BY volume, chapter_number",
            )?;
            let rows = stmt.query_map(params![sid], crate::db::models::Chapter::from_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
    }

    pub async fn get_content_chunks(&self, chapter_id: i64) -> DbResult<Vec<crate::db::models::ContentChunk>> {
        self.query(move |c| {
            let mut stmt = c.prepare(
                "SELECT id, chapter_id, paragraph_index, text, created_at FROM content_chunks WHERE chapter_id = ?1 ORDER BY paragraph_index",
            )?;
            let rows = stmt.query_map(params![chapter_id], crate::db::models::ContentChunk::from_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
    }

    pub async fn clear_content_chunks(&self, chapter_id: i64) -> DbResult<()> {
        self.query(move |c| {
            c.execute("DELETE FROM content_chunks WHERE chapter_id = ?1", params![chapter_id])?;
            Ok(())
        })
        .await
    }

    pub async fn add_content_chunk(&self, chapter_id: i64, paragraph_index: i64, text: &str) -> DbResult<()> {
        let text = text.to_string();
        self.query(move |c| {
            c.execute(
                "INSERT INTO content_chunks (chapter_id, paragraph_index, text) VALUES (?1, ?2, ?3)",
                params![chapter_id, paragraph_index, text],
            )?;
            Ok(())
        })
        .await
    }


    // ─── 向量记忆（T5-2）───

    /// 保存段落嵌入（f32 向量 → BLOB 小端字节）
    pub async fn save_chunk_embedding(&self, sid: &str, chapter_id: i64, chunk_index: i64, embedding: &[f32], text: &str) -> DbResult<()> {
        let sid = sid.to_string();
        let text = text.to_string();
        let mut bytes = Vec::with_capacity(embedding.len() * 4);
        for v in embedding {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        self.query(move |c| {
            c.execute(
                "INSERT OR REPLACE INTO content_embeddings (session_id, chapter_id, chunk_index, embedding, text) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![sid, chapter_id, chunk_index, bytes, text],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn clear_chapter_embeddings(&self, chapter_id: i64) -> DbResult<()> {
        self.query(move |c| {
            c.execute("DELETE FROM content_embeddings WHERE chapter_id = ?1", params![chapter_id])?;
            Ok(())
        })
        .await
    }

    /// 全量嵌入（检索——内存余弦——排除章节可选）
    pub async fn get_all_embeddings(&self, sid: &str, exclude_chapter_id: Option<i64>) -> DbResult<Vec<(Vec<f32>, String)>> {
        let sid = sid.to_string();
        self.query(move |c| {
            let mut rows = Vec::new();
            match exclude_chapter_id {
                Some(ex) => {
                    let mut stmt = c.prepare("SELECT embedding, text FROM content_embeddings WHERE session_id = ?1 AND chapter_id != ?2")?;
                    let q = stmt.query_map(params![sid, ex], |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?)))?;
                    for row in q {
                        let (bytes, text) = row?;
                        let v: Vec<f32> = bytes.chunks_exact(4).map(|ch| f32::from_le_bytes([ch[0], ch[1], ch[2], ch[3]])).collect();
                        rows.push((v, text));
                    }
                }
                None => {
                    let mut stmt = c.prepare("SELECT embedding, text FROM content_embeddings WHERE session_id = ?1")?;
                    let q = stmt.query_map(params![sid], |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?)))?;
                    for row in q {
                        let (bytes, text) = row?;
                        let v: Vec<f32> = bytes.chunks_exact(4).map(|ch| f32::from_le_bytes([ch[0], ch[1], ch[2], ch[3]])).collect();
                        rows.push((v, text));
                    }
                }
            }
            Ok(rows)
        })
        .await
    }


    // ─── 评审（T6）───

    /// 章节正文全文（chunks 拼接）
    pub async fn get_chapter_content_text(&self, chapter_id: i64) -> DbResult<String> {
        self.query(move |c| {
            let mut stmt = c.prepare("SELECT text FROM content_chunks WHERE chapter_id = ?1 ORDER BY paragraph_index")?;
            let rows = stmt.query_map(params![chapter_id], |r| r.get::<_, String>(0))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?.join("\n\n"))
        })
        .await
    }

    pub async fn add_review(&self, chapter_id: i64, pipeline: &str, severity: &str, location: i64, issue: &str, suggestion: &str) -> DbResult<()> {
        let (pipeline, severity, issue, suggestion) = (pipeline.to_string(), severity.to_string(), issue.to_string(), suggestion.to_string());
        self.query(move |c| {
            c.execute(
                "INSERT INTO chapter_reviews (chapter_id, pipeline, severity, location, issue, suggestion) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![chapter_id, pipeline, severity, location, issue, suggestion],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn clear_chapter_reviews(&self, chapter_id: i64) -> DbResult<()> {
        self.query(move |c| {
            c.execute("DELETE FROM chapter_reviews WHERE chapter_id = ?1", params![chapter_id])?;
            Ok(())
        })
        .await
    }

    pub async fn add_review_feedback(&self, sid: &str, chapter_id: i64, issue_type: &str, severity: &str, issue: &str, suggestion: &str) -> DbResult<i64> {
        let (sid, issue_type, severity, issue, suggestion) = (sid.to_string(), issue_type.to_string(), severity.to_string(), issue.to_string(), suggestion.to_string());
        self.query(move |c| {
            c.execute(
                "INSERT INTO review_feedback (session_id, chapter_id, issue_type, severity, issue, suggestion) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![sid, chapter_id, issue_type, severity, issue, suggestion],
            )?;
            Ok(c.last_insert_rowid())
        })
        .await
    }

    pub async fn clear_review_feedback(&self, chapter_id: i64) -> DbResult<()> {
        self.query(move |c| {
            c.execute("DELETE FROM review_feedback WHERE chapter_id = ?1", params![chapter_id])?;
            Ok(())
        })
        .await
    }

    /// 既往审查结论（rewritten 状态——教训注入）
    pub async fn get_review_feedback(&self, sid: &str, status: &str, limit: i64) -> DbResult<Vec<(String, String, String)>> {
        let (sid, status) = (sid.to_string(), status.to_string());
        self.query(move |c| {
            let mut stmt = c.prepare(
                "SELECT issue_type, issue, suggestion FROM review_feedback WHERE session_id = ?1 AND status = ?2 LIMIT ?3",
            )?;
            let rows = stmt.query_map(params![sid, status, limit], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
    }

    pub async fn mark_feedback_rewritten(&self, fb_id: i64, rewritten_text: &str) -> DbResult<()> {
        let rewritten_text = rewritten_text.to_string();
        self.query(move |c| {
            c.execute(
                "UPDATE review_feedback SET status = 'rewritten', rewritten_text = ?1 WHERE id = ?2",
                params![rewritten_text, fb_id],
            )?;
            Ok(())
        })
        .await
    }


    /// 章节评审结果（chapter_reviews——pipeline/severity/issue）
    pub async fn get_chapter_reviews(&self, chapter_id: i64) -> DbResult<Vec<(String, String, String, i64)>> {
        self.query(move |c| {
            let mut stmt = c.prepare("SELECT pipeline, severity, issue, location FROM chapter_reviews WHERE chapter_id = ?1 ORDER BY rowid")?;
            let rows = stmt.query_map(params![chapter_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_db() -> Db {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.db");
        let p = p.to_str().unwrap().to_string();
        // 保留 tempdir 存活：直接开在系统临时
        Db::open(std::path::PathBuf::from(format!("/tmp/yz_crud_test_{}.db", std::process::id()))).await.unwrap()
    }

    #[tokio::test]
    async fn session_crud_roundtrip() {
        let db = Db::open("/tmp/yz_crud_1.db").await.unwrap();
        let _ = std::fs::remove_file("/tmp/yz_crud_1.db");
        let db = Db::open("/tmp/yz_crud_1.db").await.unwrap();
        db.create_session("s1", "测试项目", "玄幻", "长篇", "zerg-ornith", "novel").await.unwrap();
        let s = db.get_session("s1").await.unwrap().expect("会话存在");
        assert_eq!(s.novel_type, "玄幻");
        assert_eq!(s.length, "长篇");
        assert_eq!(s.project_type, "novel");
        assert_eq!(s.current_block, 0);
        db.update_session_progress("s1", 3, "running").await.unwrap();
        let s2 = db.get_session("s1").await.unwrap().unwrap();
        assert_eq!(s2.current_block, 3);
        let all = db.get_all_sessions().await.unwrap();
        assert_eq!(all.len(), 1);
        std::fs::remove_file("/tmp/yz_crud_1.db").ok();
    }

    #[tokio::test]
    async fn message_and_template_flow() {
        let db = Db::open("/tmp/yz_crud_2.db").await.unwrap();
        db.create_session("s2", "", "都市", "中篇", "zerg-ornith", "novel").await.unwrap();
        // 消息
        let mid = db.add_message("s2", "司世", "世界设定草稿", "author", "").await.unwrap();
        assert!(mid > 0);
        let msgs = db.get_messages("s2", 0).await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].sender, "司世");
        // 模板 upsert + 锁
        db.upsert_template("s2", 0, "世界观", "仙侠大陆", false, "Block0").await.unwrap();
        db.upsert_template("s2", 0, "世界观", "仙侠大陆·灵气复苏", true, "Block0").await.unwrap();
        let locked = db.get_locked_field_values("s2").await.unwrap();
        assert_eq!(locked.len(), 1);
        assert_eq!(locked[0].1, "仙侠大陆·灵气复苏");
        db.lock_block("s2", "Block0").await.unwrap();
        std::fs::remove_file("/tmp/yz_crud_2.db").ok();
    }

    #[tokio::test]
    async fn world_character_tokens() {
        let db = Db::open("/tmp/yz_crud_3.db").await.unwrap();
        db.create_session("s3", "", "", "", "zerg-ornith", "novel").await.unwrap();
        db.upsert_world_setting("s3", "地理", "大陆", "九州").await.unwrap();
        db.upsert_world_setting("s3", "地理", "大陆", "十州").await.unwrap(); // 覆盖
        let ws = db.get_world_settings("s3", Some("地理"), 10).await.unwrap();
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].value.as_deref(), Some("十州"));
        db.upsert_character_state("s3", "黎渊", r#"{"hp":100}"#, 3).await.unwrap();
        let cs = db.get_character_states("s3", 10).await.unwrap();
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].last_seen_chapter, 3);
        db.add_token_usage("s3", "Block1", 100, 50).await.unwrap();
        std::fs::remove_file("/tmp/yz_crud_3.db").ok();
    }


}
