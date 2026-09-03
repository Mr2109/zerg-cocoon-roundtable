//! db/models.rs — 数据模型（T2-3——对齐 Web 版表结构）

use rusqlite::Row;
use serde::{Deserialize, Serialize};

fn row_get<T: rusqlite::types::FromSql>(r: &Row, i: usize, def: T) -> T {
    r.get::<_, T>(i).unwrap_or(def)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub novel_type: String,
    pub length: String,
    pub project_type: String,
    pub status: String,
    pub current_block: i64,
    pub state_json: Option<String>,
    pub provider: String,
    pub total_tokens: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl Session {
    pub fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(Session {
            id: r.get(0)?,
            name: row_get(r, 1, String::new()),
            novel_type: row_get(r, 2, String::new()),
            length: row_get(r, 3, String::new()),
            project_type: row_get(r, 4, "novel".to_string()),
            status: row_get(r, 5, "idle".to_string()),
            current_block: r.get(6)?,
            state_json: r.get(7)?,
            provider: row_get(r, 8, "zerg-ornith".to_string()),
            total_tokens: r.get(9)?,
            created_at: r.get(10)?,
            updated_at: r.get(11)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub session_id: String,
    pub sender: String,
    pub content: Option<String>,
    pub sender_type: String,
    pub metadata: Option<String>,
    pub created_at: String,
}

impl Message {
    pub fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(Message {
            id: r.get(0)?,
            session_id: r.get(1)?,
            sender: r.get(2)?,
            content: r.get(3)?,
            sender_type: row_get(r, 4, "author".to_string()),
            metadata: r.get(5)?,
            created_at: r.get(6)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub id: i64,
    pub session_id: String,
    pub block_index: Option<i64>,
    pub block_name: Option<String>,
    pub field_name: Option<String>,
    pub field_value: Option<String>,
    pub locked: i64,
    pub locked_at: Option<String>,
    pub score: i64,
    pub score_detail: Option<String>,
    pub created_at: String,
}

impl Template {
    pub fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(Template {
            id: r.get(0)?,
            session_id: r.get(1)?,
            block_index: r.get(2)?,
            block_name: r.get(3)?,
            field_name: r.get(4)?,
            field_value: r.get(5)?,
            locked: r.get(6)?,
            locked_at: r.get(7)?,
            score: r.get(8)?,
            score_detail: r.get(9)?,
            created_at: r.get(10)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: i64,
    pub session_id: String,
    pub volume: i64,
    pub chapter_number: i64,
    pub title: Option<String>,
    pub outline: Option<String>,
    pub content: Option<String>,
    pub locked: i64,
    pub score: i64,
    pub score_detail: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Chapter {
    pub fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(Chapter {
            id: r.get(0)?,
            session_id: r.get(1)?,
            volume: r.get(2)?,
            chapter_number: r.get(3)?,
            title: r.get(4)?,
            outline: r.get(5)?,
            content: r.get(6)?,
            locked: r.get(7)?,
            score: r.get(8)?,
            score_detail: r.get(9)?,
            created_at: r.get(10)?,
            updated_at: r.get(11)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldSetting {
    pub id: i64,
    pub session_id: String,
    pub setting_type: String,
    pub key: String,
    pub value: Option<String>,
    pub updated_at: String,
}

impl WorldSetting {
    pub fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(WorldSetting {
            id: r.get(0)?,
            session_id: r.get(1)?,
            setting_type: r.get(2)?,
            key: r.get(3)?,
            value: r.get(4)?,
            updated_at: r.get(5)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterState {
    pub id: i64,
    pub session_id: String,
    pub character_name: String,
    pub state_json: Option<String>,
    pub last_seen_chapter: i64,
    pub updated_at: String,
}

impl CharacterState {
    pub fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(CharacterState {
            id: r.get(0)?,
            session_id: r.get(1)?,
            character_name: r.get(2)?,
            state_json: r.get(3)?,
            last_seen_chapter: r.get(4)?,
            updated_at: r.get(5)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentChunk {
    pub id: i64,
    pub chapter_id: i64,
    pub paragraph_index: i64,
    pub text: String,
    pub created_at: String,
}

impl ContentChunk {
    pub fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(ContentChunk {
            id: r.get(0)?,
            chapter_id: r.get(1)?,
            paragraph_index: r.get(2)?,
            text: row_get(r, 3, String::new()),
            created_at: row_get(r, 4, String::new()),
        })
    }
}
