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
            let mut stmt = c.prepare(&format!(
                "SELECT {SESSION_COLS} FROM sessions WHERE id = ?1"
            ))?;
            let mut rows = stmt.query_map(params![id], Session::from_row)?;
            Ok(rows.next().transpose()?)
        })
        .await
    }

    pub async fn get_all_sessions(&self) -> DbResult<Vec<Session>> {
        self.query(move |c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {SESSION_COLS} FROM sessions ORDER BY created_at DESC"
            ))?;
            let rows = stmt.query_map([], Session::from_row)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
    }

    pub async fn update_session_progress(
        &self,
        id: &str,
        current_block: i64,
        status: &str,
    ) -> DbResult<()> {
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

    pub async fn update_session_tokens(
        &self,
        id: &str,
        prompt: i64,
        completion: i64,
    ) -> DbResult<()> {
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

    pub async fn get_templates(
        &self,
        sid: &str,
        block_index: Option<i64>,
    ) -> DbResult<Vec<Template>> {
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
    pub async fn upsert_world_setting(
        &self,
        sid: &str,
        setting_type: &str,
        key: &str,
        value: &str,
    ) -> DbResult<()> {
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

    pub async fn get_world_settings(
        &self,
        sid: &str,
        setting_type: Option<&str>,
        limit: i64,
    ) -> DbResult<Vec<WorldSetting>> {
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

    pub async fn get_character_states(
        &self,
        sid: &str,
        limit: i64,
    ) -> DbResult<Vec<CharacterState>> {
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
    pub async fn add_token_usage(
        &self,
        sid: &str,
        block_name: &str,
        prompt: i64,
        completion: i64,
    ) -> DbResult<()> {
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

    pub async fn create_chapter(
        &self,
        sid: &str,
        volume: i64,
        chapter_number: i64,
        title: &str,
        outline: &str,
    ) -> DbResult<()> {
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

    pub async fn update_chapter_content(
        &self,
        sid: &str,
        chapter_id: i64,
        content: &str,
    ) -> DbResult<()> {
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

    pub async fn get_content_chunks(
        &self,
        chapter_id: i64,
    ) -> DbResult<Vec<crate::db::models::ContentChunk>> {
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
            c.execute(
                "DELETE FROM content_chunks WHERE chapter_id = ?1",
                params![chapter_id],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn add_content_chunk(
        &self,
        chapter_id: i64,
        paragraph_index: i64,
        text: &str,
    ) -> DbResult<()> {
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
    pub async fn save_chunk_embedding(
        &self,
        sid: &str,
        chapter_id: i64,
        chunk_index: i64,
        embedding: &[f32],
        text: &str,
    ) -> DbResult<()> {
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
            c.execute(
                "DELETE FROM content_embeddings WHERE chapter_id = ?1",
                params![chapter_id],
            )?;
            Ok(())
        })
        .await
    }

    /// 全量嵌入（检索——内存余弦——排除章节可选）
    pub async fn get_all_embeddings(
        &self,
        sid: &str,
        exclude_chapter_id: Option<i64>,
    ) -> DbResult<Vec<(Vec<f32>, String)>> {
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
            let mut stmt = c.prepare(
                "SELECT text FROM content_chunks WHERE chapter_id = ?1 ORDER BY paragraph_index",
            )?;
            let rows = stmt.query_map(params![chapter_id], |r| r.get::<_, String>(0))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?.join("\n\n"))
        })
        .await
    }

    pub async fn add_review(
        &self,
        chapter_id: i64,
        pipeline: &str,
        severity: &str,
        location: i64,
        issue: &str,
        suggestion: &str,
    ) -> DbResult<()> {
        let (pipeline, severity, issue, suggestion) = (
            pipeline.to_string(),
            severity.to_string(),
            issue.to_string(),
            suggestion.to_string(),
        );
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
            c.execute(
                "DELETE FROM chapter_reviews WHERE chapter_id = ?1",
                params![chapter_id],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn add_review_feedback(
        &self,
        sid: &str,
        chapter_id: i64,
        issue_type: &str,
        severity: &str,
        issue: &str,
        suggestion: &str,
    ) -> DbResult<i64> {
        let (sid, issue_type, severity, issue, suggestion) = (
            sid.to_string(),
            issue_type.to_string(),
            severity.to_string(),
            issue.to_string(),
            suggestion.to_string(),
        );
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
            c.execute(
                "DELETE FROM review_feedback WHERE chapter_id = ?1",
                params![chapter_id],
            )?;
            Ok(())
        })
        .await
    }

    /// 既往审查结论（rewritten 状态——教训注入）
    pub async fn get_review_feedback(
        &self,
        sid: &str,
        status: &str,
        limit: i64,
    ) -> DbResult<Vec<(String, String, String)>> {
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
    pub async fn get_chapter_reviews(
        &self,
        chapter_id: i64,
    ) -> DbResult<Vec<(String, String, String, i64)>> {
        self.query(move |c| {
            let mut stmt = c.prepare("SELECT pipeline, severity, issue, location FROM chapter_reviews WHERE chapter_id = ?1 ORDER BY rowid")?;
            let rows = stmt.query_map(params![chapter_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
    }

    // ── errors 表（L2——日志/报错体系 2026-09-04——WARN/ERROR 镜像入库）──

    /// 写入错误记录（logger 写线程调用——非 async 上下文用 spawn_blocking）
    pub fn insert_error_sync(
        &self,
        ts: &str,
        level: &str,
        module: &str,
        sid: Option<&str>,
        block: Option<&str>,
        kind: &str,
        code: Option<&str>,
        msg: &str,
        detail: Option<&str>,
    ) {
        let (ts, level, module, sid, block, kind, code, msg, detail) = (
            ts.to_string(),
            level.to_string(),
            module.to_string(),
            sid.map(|s| s.to_string()),
            block.map(|s| s.to_string()),
            kind.to_string(),
            code.map(|s| s.to_string()),
            msg.to_string(),
            detail.map(|s| s.to_string()),
        );
        let _ = self.call_sync(move |c| {
            c.execute(
                "INSERT INTO errors (ts, level, module, sid, block, kind, code, msg, detail) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![ts, level, module, sid, block, kind, code, msg, detail],
            )?;
            Ok(())
        });
    }

    // ── flow_vars 变量池（A3——v1.0.1——selector=(node_id,key)——UNIQUE 覆写）──

    /// 写变量（节点锁定时调用——同 selector 覆写——graphon VariablePool.add 语义）
    pub async fn set_flow_var(
        &self,
        sid: &str,
        node_id: &str,
        key: &str,
        value: &str,
    ) -> DbResult<()> {
        let (sid, node_id, key, value) = (
            sid.to_string(),
            node_id.to_string(),
            key.to_string(),
            value.to_string(),
        );
        self.call(move |c| {
            c.execute(
                "INSERT INTO flow_vars (sid, node_id, key, value) VALUES (?1,?2,?3,?4)
                 ON CONFLICT(sid, node_id, key) DO UPDATE SET value = excluded.value, ts = datetime('now')",
                params![sid, node_id, key, value],
            )?;
            Ok(())
        })
        .await
    }

    /// 读单变量（{{node.key}} 替换用）
    pub async fn get_flow_var(
        &self,
        sid: &str,
        node_id: &str,
        key: &str,
    ) -> DbResult<Option<String>> {
        let (sid, node_id, key) = (sid.to_string(), node_id.to_string(), key.to_string());
        self.query(move |c| {
            let v = match c.query_row(
                "SELECT value FROM flow_vars WHERE sid=?1 AND node_id=?2 AND key=?3",
                params![sid, node_id, key],
                |r| r.get::<_, String>(0),
            ) {
                Ok(v) => Some(v),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(e.into()),
            };
            Ok(v)
        })
        .await
    }

    /// 读会话全部变量（上下文组装——{{}} 批量替换）
    pub async fn get_flow_vars(&self, sid: &str) -> DbResult<Vec<(String, String, String)>> {
        let sid = sid.to_string();
        self.query(move |c| {
            let mut stmt =
                c.prepare("SELECT node_id, key, value FROM flow_vars WHERE sid=?1 ORDER BY id")?;
            let rows = stmt.query_map(params![sid], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
    }

    /// 错误列表（UI 日志对话框——最新在前——limit 200）
    pub async fn list_errors(&self, sid: Option<&str>, limit: i64) -> DbResult<Vec<ErrorRow>> {
        let (sid, limit) = (sid.map(|s| s.to_string()), limit);
        self.query(move |c| {
            let mut stmt = c.prepare(
                "SELECT ts, level, module, sid, block, kind, code, msg, detail FROM errors WHERE (?1 IS NULL OR sid = ?1) ORDER BY id DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![sid, limit], |r| {
                Ok(ErrorRow {
                    ts: r.get(0)?,
                    level: r.get(1)?,
                    module: r.get(2)?,
                    sid: r.get(3)?,
                    block: r.get(4)?,
                    kind: r.get(5)?,
                    code: r.get(6)?,
                    msg: r.get(7)?,
                    detail: r.get(8)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .await
    }
}

/// 错误行（UI 日志对话框行数据）
#[derive(Debug, Clone)]
pub struct ErrorRow {
    pub ts: String,
    pub level: String,
    pub module: String,
    pub sid: Option<String>,
    pub block: Option<String>,
    pub kind: String,
    pub code: Option<String>,
    pub msg: String,
    pub detail: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_db() -> Db {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.db");
        let p = p.to_str().unwrap().to_string();
        // 保留 tempdir 存活：直接开在系统临时
        Db::open(std::path::PathBuf::from(format!(
            "/tmp/yz_crud_test_{}.db",
            std::process::id()
        )))
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn session_crud_roundtrip() {
        let db = Db::open("/tmp/yz_crud_1.db").await.unwrap();
        let _ = std::fs::remove_file("/tmp/yz_crud_1.db");
        let db = Db::open("/tmp/yz_crud_1.db").await.unwrap();
        db.create_session("s1", "测试项目", "玄幻", "长篇", "zerg-ornith", "novel")
            .await
            .unwrap();
        let s = db.get_session("s1").await.unwrap().expect("会话存在");
        assert_eq!(s.novel_type, "玄幻");
        assert_eq!(s.length, "长篇");
        assert_eq!(s.project_type, "novel");
        assert_eq!(s.current_block, 0);
        db.update_session_progress("s1", 3, "running")
            .await
            .unwrap();
        let s2 = db.get_session("s1").await.unwrap().unwrap();
        assert_eq!(s2.current_block, 3);
        let all = db.get_all_sessions().await.unwrap();
        assert_eq!(all.len(), 1);
        std::fs::remove_file("/tmp/yz_crud_1.db").ok();
    }

    #[tokio::test]
    async fn message_and_template_flow() {
        let db = Db::open("/tmp/yz_crud_2.db").await.unwrap();
        db.create_session("s2", "", "都市", "中篇", "zerg-ornith", "novel")
            .await
            .unwrap();
        // 消息
        let mid = db
            .add_message("s2", "司世", "世界设定草稿", "author", "")
            .await
            .unwrap();
        assert!(mid > 0);
        let msgs = db.get_messages("s2", 0).await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].sender, "司世");
        // 模板 upsert + 锁
        db.upsert_template("s2", 0, "世界观", "仙侠大陆", false, "Block0")
            .await
            .unwrap();
        db.upsert_template("s2", 0, "世界观", "仙侠大陆·灵气复苏", true, "Block0")
            .await
            .unwrap();
        let locked = db.get_locked_field_values("s2").await.unwrap();
        assert_eq!(locked.len(), 1);
        assert_eq!(locked[0].1, "仙侠大陆·灵气复苏");
        db.lock_block("s2", "Block0").await.unwrap();
        std::fs::remove_file("/tmp/yz_crud_2.db").ok();
    }

    #[tokio::test]
    async fn world_character_tokens() {
        let db = Db::open("/tmp/yz_crud_3.db").await.unwrap();
        db.create_session("s3", "", "", "", "zerg-ornith", "novel")
            .await
            .unwrap();
        db.upsert_world_setting("s3", "地理", "大陆", "九州")
            .await
            .unwrap();
        db.upsert_world_setting("s3", "地理", "大陆", "十州")
            .await
            .unwrap(); // 覆盖
        let ws = db.get_world_settings("s3", Some("地理"), 10).await.unwrap();
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].value.as_deref(), Some("十州"));
        db.upsert_character_state("s3", "黎渊", r#"{"hp":100}"#, 3)
            .await
            .unwrap();
        let cs = db.get_character_states("s3", 10).await.unwrap();
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].last_seen_chapter, 3);
        db.add_token_usage("s3", "Block1", 100, 50).await.unwrap();
        std::fs::remove_file("/tmp/yz_crud_3.db").ok();
    }

    #[tokio::test]
    async fn flow_vars_roundtrip() {
        let db = Db::open(format!("/tmp/yz_flowvar_{}.db", std::process::id()))
            .await
            .unwrap();
        db.set_flow_var("rt1", "n0", "故事核", "少年修仙复仇")
            .await
            .unwrap();
        // 覆写（UNIQUE upsert）
        db.set_flow_var("rt1", "n0", "故事核", "少年修仙复仇（终）")
            .await
            .unwrap();
        let v = db.get_flow_var("rt1", "n0", "故事核").await.unwrap();
        assert_eq!(v.as_deref(), Some("少年修仙复仇（终）"));
        // 全量读（顺序稳定）
        db.set_flow_var("rt1", "n1", "结论", "复仇线为主")
            .await
            .unwrap();
        let all = db.get_flow_vars("rt1").await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].0, "n0");
        // 会话隔离
        let other = db.get_flow_vars("rt2").await.unwrap();
        assert!(other.is_empty());
        // 未定义引用返回 None
        let miss = db.get_flow_var("rt1", "nX", "y").await.unwrap();
        assert!(miss.is_none());
        std::fs::remove_file(format!("/tmp/yz_flowvar_{}.db", std::process::id())).ok();
    }

    #[tokio::test]
    async fn errors_roundtrip() {
        let db = Db::open(format!("/tmp/yz_crud_err_{}.db", std::process::id()))
            .await
            .unwrap();
        db.insert_error_sync(
            "2026-09-04T02:00:00+08:00",
            "WARN",
            "engine::run",
            Some("rt_test1"),
            Some("核心冲突"),
            "env",
            Some("E102"),
            "AI 重试 2/3：HTTP 429",
            None,
        );
        db.insert_error_sync(
            "2026-09-04T02:01:00+08:00",
            "ERROR",
            "ui::session",
            None,
            None,
            "bug",
            Some("B101"),
            "状态机非法迁移",
            Some("detail: idle→locked"),
        );
        let all = db.list_errors(None, 200).await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].code.as_deref(), Some("B101")); // 最新在前
        let one = db.list_errors(Some("rt_test1"), 200).await.unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].kind, "env");
        std::fs::remove_file(format!("/tmp/yz_crud_err_{}.db", std::process::id())).ok();
    }

    // ─── T9-1：Web 版 test_discussion.py 用例移植 ───
    // 命名保留 Web 侧用例语义，注释标出出处；两版**有差异**的地方就地注明（不假装等价）。

    #[tokio::test]
    async fn get_nonexistent_session_returns_none() {
        // T9-1 ← Web::test_get_nonexistent_session
        let p = format!("/tmp/yz_t9_none_{}.db", std::process::id());
        let db = Db::open(p.clone()).await.unwrap();
        assert!(db.get_session("nonexistent").await.unwrap().is_none());
        std::fs::remove_file(&p).ok();
    }

    #[tokio::test]
    async fn update_progress_sets_block_and_status() {
        // T9-1 ← Web::test_update_progress（Web 同时断言 current_block 与 status 两项）
        let p = format!("/tmp/yz_t9_prog_{}.db", std::process::id());
        let db = Db::open(p.clone()).await.unwrap();
        db.create_session("t9p", "", "玄幻", "长篇", "zerg-ornith", "novel")
            .await
            .unwrap();
        db.update_session_progress("t9p", 5, "running").await.unwrap();
        let s = db.get_session("t9p").await.unwrap().unwrap();
        assert_eq!(s.current_block, 5);
        assert_eq!(s.status, "running");
        std::fs::remove_file(&p).ok();
    }

    #[tokio::test]
    async fn messages_count_and_paging() {
        // T9-1 ← Web::test_get_messages（3 条消息 + after_id 分页）
        let p = format!("/tmp/yz_t9_msg_{}.db", std::process::id());
        let db = Db::open(p.clone()).await.unwrap();
        db.create_session("t9m", "", "", "", "zerg-ornith", "novel")
            .await
            .unwrap();
        let m1 = db.add_message("t9m", "司世", "消息1", "author", "").await.unwrap();
        db.add_message("t9m", "司人", "消息2", "author", "").await.unwrap();
        db.add_message("t9m", "主持人", "消息3", "moderator", "").await.unwrap();
        assert!(m1 > 0);
        assert_eq!(db.get_all_messages("t9m").await.unwrap().len(), 3);
        let after = db.get_messages("t9m", m1).await.unwrap();
        assert!(!after.is_empty(), "after_id 之后应仍有消息");
        assert!(after.iter().all(|m| m.id > m1), "分页结果必须都晚于 after_id");
        std::fs::remove_file(&p).ok();
    }

    #[tokio::test]
    async fn locked_values_exclude_unlocked() {
        // T9-1 ← Web::test_get_locked_field_values（多字段锁定 + 未锁定字段不得出现）
        let p = format!("/tmp/yz_t9_lk_{}.db", std::process::id());
        let db = Db::open(p.clone()).await.unwrap();
        db.create_session("t9l", "", "", "", "zerg-ornith", "novel")
            .await
            .unwrap();
        db.upsert_template("t9l", 0, "类型", "玄幻", true, "类型").await.unwrap();
        db.upsert_template("t9l", 1, "故事核", "复仇", true, "故事核").await.unwrap();
        db.upsert_template("t9l", 0, "未锁定字段", "值", false, "类型").await.unwrap();
        let locked = db.get_locked_field_values("t9l").await.unwrap();
        let map: std::collections::HashMap<String, String> = locked.into_iter().collect();
        assert_eq!(map.get("类型").map(String::as_str), Some("玄幻"));
        assert_eq!(map.get("故事核").map(String::as_str), Some("复仇"));
        assert!(!map.contains_key("未锁定字段"), "未锁定字段不得进入锁定值集合");
        std::fs::remove_file(&p).ok();
    }

    #[tokio::test]
    async fn chapters_create_get_order() {
        // T9-1 ← Web::test_create_chapter + test_get_chapters（三章、跨卷）
        // 差异：Rust 版 create_chapter 返回 ()（Web 返回自增 id）⇒ 以「查得回 + 排序」作等价断言。
        let p = format!("/tmp/yz_t9_ch_{}.db", std::process::id());
        let db = Db::open(p.clone()).await.unwrap();
        db.create_session("t9c", "", "", "", "zerg-ornith", "novel")
            .await
            .unwrap();
        db.create_chapter("t9c", 1, 1, "第一章", "大纲1").await.unwrap();
        db.create_chapter("t9c", 1, 2, "第二章", "大纲2").await.unwrap();
        db.create_chapter("t9c", 2, 1, "第三卷第一章", "大纲3").await.unwrap();
        let chs = db.get_chapters("t9c").await.unwrap();
        assert_eq!(chs.len(), 3);
        assert_eq!((chs[0].volume, chs[0].chapter_number), (1, 1), "按 volume,chapter_number 排序");
        assert_eq!((chs[2].volume, chs[2].chapter_number), (2, 1));
        assert_eq!(chs[0].title.as_deref(), Some("第一章"));
        assert_eq!(chs[0].outline.as_deref(), Some("大纲1"));
        // 正文更新（Rust 版章节的唯一更新入口；Web 的 update_chapter 改的是 title/outline）
        let cid = chs[0].id;
        db.update_chapter_content("t9c", cid, "正文内容").await.unwrap();
        let chs2 = db.get_chapters("t9c").await.unwrap();
        let one = chs2.iter().find(|c| c.id == cid).unwrap();
        assert_eq!(one.content.as_deref(), Some("正文内容"));
        std::fs::remove_file(&p).ok();
    }

    #[tokio::test]
    async fn full_flow_lock_all_fields_then_chapter() {
        // T9-1 ← Web::TestBlockFlow::test_full_flow（前 5 Block 全字段锁定 ⇒ 数量相等 + 章节建/查）
        let p = format!("/tmp/yz_t9_flow_{}.db", std::process::id());
        let db = Db::open(p.clone()).await.unwrap();
        db.create_session("t9f", "测试流程", "玄幻", "长篇", "zerg-ornith", "novel")
            .await
            .unwrap();
        let mut expect = 0usize;
        for (i, b) in crate::templates::novel::NOVEL_BLOCKS.iter().take(5).enumerate() {
            for f in &b.fields {
                db.upsert_template("t9f", i as i64, f, &format!("测试_{}", f), true, &b.name)
                    .await
                    .unwrap();
                expect += 1;
            }
        }
        let locked = db.get_locked_field_values("t9f").await.unwrap();
        assert_eq!(locked.len(), expect, "前 5 Block 的全部字段都应锁定");
        db.create_chapter("t9f", 1, 1, "测试章", "测试大纲").await.unwrap();
        assert_eq!(db.get_chapters("t9f").await.unwrap().len(), 1);
        std::fs::remove_file(&p).ok();
    }
}
