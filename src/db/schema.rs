//! db/schema.rs — 圆桌派 Rust 版数据库 schema（T2-1——2026-09-03）
//! 来源：Web 版 database.py init_db（10 表）——Rust 化——差异：
//! ① sessions 加 project_type（项目模板选择——引擎/模板分离——默认 'novel'）
//! ② 字段类型对齐 SQLite（rusqlite 同 SQL 语法）

/// 建表 SQL（10 表 + 索引——对齐 Web 版 + project_type）
pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    name TEXT,
    novel_type TEXT,           -- 小说类型（玄幻/都市/…——人工创建时定——M1）
    length TEXT,               -- 字数规模（长篇/中篇/…——人工创建时定——M1）
    project_type TEXT DEFAULT 'novel',  -- 项目模板类型（引擎/模板分离——未来剧本/方案）
    status TEXT DEFAULT 'idle',
    current_block INTEGER DEFAULT 0,
    state_json TEXT,
    provider TEXT DEFAULT 'zerg-ornith',
    total_tokens INTEGER DEFAULT 0,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    content TEXT,
    sender_type TEXT DEFAULT 'author',
    metadata TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

CREATE TABLE IF NOT EXISTS templates (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    block_index INTEGER,
    block_name TEXT,
    field_name TEXT,
    field_value TEXT,
    locked INTEGER DEFAULT 0,
    locked_at TEXT,
    score INTEGER DEFAULT 0,
    score_detail TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (session_id) REFERENCES sessions(id),
    UNIQUE(session_id, block_index, field_name)
);

CREATE TABLE IF NOT EXISTS chapters (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    volume INTEGER NOT NULL,
    chapter_number INTEGER NOT NULL,
    title TEXT,
    outline TEXT,
    content TEXT,
    locked INTEGER DEFAULT 0,
    locked_at TEXT,
    score INTEGER DEFAULT 0,
    score_detail TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (session_id) REFERENCES sessions(id),
    UNIQUE(session_id, volume, chapter_number)
);

CREATE TABLE IF NOT EXISTS content_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chapter_id INTEGER NOT NULL,
    paragraph_index INTEGER NOT NULL,
    text TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (chapter_id) REFERENCES chapters(id)
);

CREATE TABLE IF NOT EXISTS discussions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    block_index INTEGER,
    block_name TEXT,
    round INTEGER,
    status TEXT DEFAULT 'active',
    result TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

CREATE TABLE IF NOT EXISTS token_usage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    block_name TEXT,
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
CREATE INDEX IF NOT EXISTS idx_chapters_session ON chapters(session_id);
CREATE INDEX IF NOT EXISTS idx_chapters_volume ON chapters(session_id, volume);
CREATE INDEX IF NOT EXISTS idx_templates_session ON templates(session_id);
CREATE INDEX IF NOT EXISTS idx_discussions_session ON discussions(session_id);

CREATE TABLE IF NOT EXISTS world_settings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    setting_type TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT,
    updated_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (session_id) REFERENCES sessions(id),
    UNIQUE(session_id, setting_type, key)
);

CREATE TABLE IF NOT EXISTS character_state (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    character_name TEXT NOT NULL,
    state_json TEXT,
    last_seen_chapter INTEGER DEFAULT 0,
    updated_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (session_id) REFERENCES sessions(id),
    UNIQUE(session_id, character_name)
);

CREATE TABLE IF NOT EXISTS content_embeddings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    chapter_id INTEGER NOT NULL,
    chunk_index INTEGER NOT NULL,
    embedding BLOB,
    text TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (chapter_id) REFERENCES chapters(id),
    UNIQUE(chapter_id, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_world_settings_session ON world_settings(session_id);
CREATE INDEX IF NOT EXISTS idx_character_state_session ON character_state(session_id);
CREATE INDEX IF NOT EXISTS idx_content_embeddings_session ON content_embeddings(session_id);

CREATE TABLE IF NOT EXISTS chapter_reviews (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chapter_id INTEGER NOT NULL,
    pipeline TEXT,
    severity TEXT,
    location INTEGER,
    issue TEXT,
    suggestion TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (chapter_id) REFERENCES chapters(id)
);
CREATE TABLE IF NOT EXISTS review_feedback (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    chapter_id INTEGER NOT NULL,
    issue_type TEXT,
    severity TEXT,
    issue TEXT,
    suggestion TEXT,
    status TEXT DEFAULT 'pending',
    rewritten_text TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (chapter_id) REFERENCES chapters(id)
);
CREATE INDEX IF NOT EXISTS idx_reviews_chapter ON chapter_reviews(chapter_id);
CREATE INDEX IF NOT EXISTS idx_feedback_session ON review_feedback(session_id);
CREATE TABLE IF NOT EXISTS errors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts TEXT NOT NULL,
    level TEXT NOT NULL,
    module TEXT NOT NULL,
    sid TEXT,
    block TEXT,
    kind TEXT NOT NULL,
    code TEXT,
    msg TEXT NOT NULL,
    detail TEXT
);
CREATE INDEX IF NOT EXISTS idx_errors_sid ON errors(sid);
"#;

/// 初始化数据库（建表）
pub fn init_db(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA_SQL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_creates_all_tables() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        // 10 表验证
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let expect = [
            "chapters",
            "character_state",
            "content_chunks",
            "content_embeddings",
            "discussions",
            "messages",
            "sessions",
            "templates",
            "token_usage",
            "world_settings",
        ];
        assert_eq!(
            tables.len(),
            13,
            "表数应为 13（errors 表——L2）——实际: {tables:?}"
        );
        for e in expect {
            assert!(tables.contains(&e.to_string()), "缺表: {e}");
        }
        // sessions 有 project_type 字段
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(sessions)")
            .unwrap()
            .query_map([], |r| r.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(
            cols.contains(&"project_type".to_string()),
            "缺 project_type"
        );
    }
}
