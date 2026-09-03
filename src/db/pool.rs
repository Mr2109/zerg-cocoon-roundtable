//! db/pool.rs — rusqlite 线程隔离层（T2-2——2026-09-03）
//! 模式：std::sync::Mutex<Connection>（Arc 共享）+ tokio::task::spawn_blocking
//! 关键：锁在 spawn_blocking 内获取（阻塞线程里——不跨 await 持锁——MutexGuard 不外逃）
//!       rusqlite 同步阻塞——spawn_blocking 让 SQL 跑专用线程池——async 侧 await 安全

use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// 异步安全的数据库句柄（连接单例——串行访问）
#[derive(Clone)]
pub struct Db {
    inner: Arc<Mutex<Connection>>,
}

pub type DbResult<T> = Result<T, DbError>;

#[derive(thiserror::Error, Debug)]
pub enum DbError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("任务失败: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("锁中毒: {0}")]
    Poison(String),
}

impl Db {
    /// 打开（或创建）数据库并初始化 schema
    pub async fn open(path: impl AsRef<Path> + Send + 'static) -> DbResult<Self> {
        let p = path.as_ref().to_path_buf();
        let conn = tokio::task::spawn_blocking(move || -> rusqlite::Result<Connection> {
            let conn = Connection::open(&p)?;
            crate::db::schema::init_db(&conn)?;
            Ok(conn)
        })
        .await??;
        Ok(Db {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    /// 在专用线程执行同步 DB 写操作（async 安全）
    pub async fn call<T, F>(&self, f: F) -> DbResult<T>
    where
        F: FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || -> DbResult<T> {
            let mut conn = inner.lock().map_err(|e| DbError::Poison(e.to_string()))?;
            f(&mut conn).map_err(DbError::from)
        })
        .await
        .map_err(DbError::from)?
    }

    /// 只读查询（同 call——语义标注）
    pub async fn query<T, F>(&self, f: F) -> DbResult<T>
    where
        F: FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || -> DbResult<T> {
            let conn = inner.lock().map_err(|e| DbError::Poison(e.to_string()))?;
            f(&conn).map_err(DbError::from)
        })
        .await
        .map_err(DbError::from)?
    }

    /// 同步执行（仅供专职写线程——logger 落库——无 runtime 上下文；阻塞当前线程直到完成）
    pub fn call_sync<T, F>(&self, f: F) -> DbResult<T>
    where
        F: FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let inner = Arc::clone(&self.inner);
        let mut conn = inner.lock().map_err(|e| DbError::Poison(e.to_string()))?;
        f(&mut conn).map_err(DbError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn open_creates_schema() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path().join("test.db")).await.unwrap();
        let tables: Vec<String> = db
            .query(|c| {
                let mut stmt = c
                    .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
                    .unwrap();
                let v = stmt
                    .query_map([], |r| r.get(0))
                    .unwrap()
                    .collect::<Result<Vec<String>, _>>()
                    .unwrap();
                Ok(v)
            })
            .await
            .unwrap();
        assert_eq!(tables.len(), 13, "13 表（errors——L2）——实际 {tables:?}");
    }

    #[tokio::test]
    async fn concurrent_calls_serialize() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path().join("test.db")).await.unwrap();
        let mut handles = Vec::new();
        for i in 0..10 {
            let db = db.clone();
            handles.push(tokio::spawn(async move {
                db.call(move |c| {
                    c.execute(
                        "INSERT INTO sessions (id, name) VALUES (?1, ?2)",
                        rusqlite::params![format!("s{i}"), "test"],
                    )
                })
                .await
                .unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        let n: i64 = db
            .query(|c| {
                Ok(
                    c.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
                        .unwrap(),
                )
            })
            .await
            .unwrap();
        assert_eq!(n, 10, "10 并发写全部成功");
        // 锁不长期占用——再次调用立即可进
        let again: i64 = db
            .query(|c| {
                Ok(
                    c.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
                        .unwrap(),
                )
            })
            .await
            .unwrap();
        assert_eq!(again, 10);
    }
}
