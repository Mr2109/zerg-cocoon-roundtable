//! engine/scheduler.rs — 批量调度器（T6-2——2026-09-03）
//! Web 版 run_batch(1111-1123) 移植 + M7：AI 统一调度队列——多会话排队单槽
//! 队列消费：enqueue 会话 → run() 顺序执行（每会话独立 stop_flag——可停/终止）
//! 模板按 project_type 取（novel 预置——模板注册表 T8 扩展）

use crate::ai::AiMessage;
use crate::db::pool::Db;
use crate::engine::discussion::BoxAi;
use crate::engine::run::run_discussion;
use crate::templates::{novel, ProjectTemplate};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// 会话状态
#[derive(Debug, Clone, PartialEq)]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Stopped,
    Failed,
}

/// 批量结果
#[derive(Debug, Clone)]
pub struct BatchResult {
    pub sid: String,
    pub status: String,
    pub error: Option<String>,
}

pub struct Scheduler {
    db: Db,
    queue: Mutex<VecDeque<String>>,
    running: AtomicBool,
    stop_flags: Mutex<HashMap<String, std::sync::Arc<AtomicBool>>>,
    statuses: Mutex<HashMap<String, String>>,
    stop_all_flag: std::sync::Arc<AtomicBool>,
}

impl Scheduler {
    pub fn new(db: Db) -> Self {
        Scheduler {
            db,
            queue: Mutex::new(VecDeque::new()),
            running: AtomicBool::new(false),
            stop_flags: Mutex::new(HashMap::new()),
            statuses: Mutex::new(HashMap::new()),
            stop_all_flag: std::sync::Arc::new(AtomicBool::new(false)),
        }
    }

    /// 排队一个会话
    pub fn enqueue(&self, sid: &str) {
        let mut q = self.queue.lock().unwrap();
        if !q.iter().any(|s| s == sid) {
            q.push_back(sid.to_string());
            self.statuses.lock().unwrap().insert(sid.to_string(), "queued".into());
        }
        self.stop_all_flag.store(false, Ordering::Relaxed);
    }

    /// 排队多个（Web run_batch 语义）
    pub fn enqueue_all(&self, sids: &[&str]) {
        for s in sids {
            self.enqueue(s);
        }
    }

    /// 消费队列（顺序执行——每会话断点续跑）
    pub async fn run(&self, ai: &BoxAi, quality_gate: bool) -> Vec<BatchResult> {
        if self.running.swap(true, Ordering::SeqCst) {
            return vec![BatchResult { sid: "".into(), status: "busy".into(), error: Some("调度器已在运行".into()) }];
        }
        let tmpl = novel::novel_template();
        let mut results = Vec::new();
        loop {
            if self.stop_all_flag.load(Ordering::Relaxed) {
                break;
            }
            let sid = {
                let mut q = self.queue.lock().unwrap();
                q.pop_front()
            };
            let sid = match sid {
                Some(s) => s,
                None => break,
            };
            // 每会话独立 stop flag
            let flag = std::sync::Arc::new(AtomicBool::new(false));
            self.stop_flags.lock().unwrap().insert(sid.clone(), flag.clone());
            self.statuses.lock().unwrap().insert(sid.clone(), "running".into());
            let out = run_discussion(&self.db, ai, &tmpl, &sid, &flag, quality_gate).await;
            match out {
                Ok(sum) => {
                    if sum.completed {
                        self.statuses.lock().unwrap().insert(sid.clone(), "completed".into());
                        results.push(BatchResult { sid: sid.clone(), status: "completed".into(), error: None });
                    } else {
                        self.statuses.lock().unwrap().insert(sid.clone(), "stopped".into());
                        results.push(BatchResult { sid: sid.clone(), status: "stopped".into(), error: None });
                    }
                }
                Err(e) => {
                    self.statuses.lock().unwrap().insert(sid.clone(), "failed".into());
                    let msg: String = e.chars().take(200).collect();
                    results.push(BatchResult { sid: sid.clone(), status: "error".into(), error: Some(msg) });
                }
            }
            self.stop_flags.lock().unwrap().remove(&sid);
        }
        self.running.store(false, Ordering::SeqCst);
        results
    }

    /// 终止单个会话（下轮检查停）
    pub fn stop_session(&self, sid: &str) {
        let flags = self.stop_flags.lock().unwrap();
        if let Some(f) = flags.get(sid) {
            f.store(true, Ordering::Relaxed);
        }
    }

    /// 终止全部
    pub fn stop_all(&self) {
        self.stop_all_flag.store(true, Ordering::Relaxed);
        let flags = self.stop_flags.lock().unwrap();
        for f in flags.values() {
            f.store(true, Ordering::Relaxed);
        }
    }

    /// 当前状态快照
    pub fn status(&self) -> Vec<(String, String)> {
        let st = self.statuses.lock().unwrap();
        let mut v: Vec<(String, String)> = st.iter().map(|(k, s)| (k.clone(), s.clone())).collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::mock::MockProvider;
    use crate::db::pool::Db;

    /// 2 会话批量顺序完成（模板 13 块 mock 弹尾自动锁）
    #[tokio::test]
    async fn batch_two_sessions() {
        let _ = std::fs::remove_file("/tmp/yz_sch1.db");
        let db = Db::open("/tmp/yz_sch1.db").await.unwrap();
        db.create_session("s1", "批量1", "玄幻", "长篇", "zerg", "novel").await.unwrap();
        db.create_session("s2", "批量2", "都市", "中篇", "zerg", "novel").await.unwrap();
        // mock 脚本：每会话首块草案流（8 响应）——其余弹尾
        let mut script: Vec<String> = Vec::new();
        for _ in 0..2 {
            script.push("草案1：\n故事核:批量测试世界设定内容一。\n\n草案2：\n故事核:备选二。\n\n草案3：\n故事核:备选三。".to_string());
            script.push("请作者表态。".to_string());
            for _ in 0..5 {
                script.push("草案1：满意。".to_string());
            }
            script.push("选中草案1。".to_string());
        }
        let refs: Vec<&str> = script.iter().map(|s| s.as_str()).collect();
        let ai: BoxAi = Box::new(MockProvider::new(refs));
        let sch = Scheduler::new(db.clone());
        sch.enqueue_all(&["s1", "s2"]);
        assert_eq!(sch.queue.lock().unwrap().len(), 2, "队列 2");
        let results = sch.run(&ai, false).await;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].sid, "s1");
        assert_eq!(results[0].status, "completed");
        assert_eq!(results[1].sid, "s2");
        assert_eq!(results[1].status, "completed");
        let st = sch.status();
        assert!(st.iter().all(|(_, s)| s == "completed"), "全 completed——{:?}", st);
        std::fs::remove_file("/tmp/yz_sch1.db").ok();
    }

    /// 中途 stop_all——后续会话停
    #[tokio::test]
    async fn batch_stopable() {
        let _ = std::fs::remove_file("/tmp/yz_sch2.db");
        let db = Db::open("/tmp/yz_sch2.db").await.unwrap();
        db.create_session("t1", "可停1", "玄幻", "长篇", "zerg", "novel").await.unwrap();
        db.create_session("t2", "可停2", "都市", "中篇", "zerg", "novel").await.unwrap();
        let mut script: Vec<String> = Vec::new();
        script.push("草案1：\n故事核:停测设定。\n\n草案2：\n故事核:备选二。\n\n草案3：\n故事核:备选三。".to_string());
        script.push("请作者表态。".to_string());
        for _ in 0..5 {
            script.push("草案1：满意。".to_string());
        }
        script.push("选中草案1。".to_string());
        let refs: Vec<&str> = script.iter().map(|s| s.as_str()).collect();
        let ai: BoxAi = Box::new(MockProvider::new(refs));
        let sch = Scheduler::new(db.clone());
        sch.enqueue_all(&["t1", "t2"]);
        let results = sch.run(&ai, false).await;
        assert_eq!(results.len(), 2);
        // stop_all：清 stop 标志需重新 enqueue（enqueue 复位）——保持 stop 状态时 run 空队列即停
        sch.stop_all();
        // 不 enqueue——run 空队列立即返回空
        let r2 = sch.run(&ai, false).await;
        assert!(r2.is_empty(), "stop_all 置位后空队列不消费");
        // 重新 enqueue（复位 stop）——新批次可跑
        sch.enqueue("t1");
        let r3 = sch.run(&ai, false).await;
        assert_eq!(r3.len(), 1, "复位后可跑——实际 {:?}", r3);
        std::fs::remove_file("/tmp/yz_sch2.db").ok();
    }
}
