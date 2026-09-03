//! logging.rs — 圆桌派日志系统（L1——2026-09-04 设计定稿）
//! 双落盘：JSONL 按天滚动文件（保留 15 天）+ errors 表（WARN/ERROR 镜像——L2 表）
//! 架构：实现 log::Log 标准挂点——内部 mpsc + 专职写线程——业务线程只 send 不阻塞
//! 业务代码只用 log::info!/warn!/error!——不感知落盘细节

use chrono::{Datelike, Local, Timelike};
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

/// 写线程消息——文件行（入库由同一线程判级完成——L2 后接 errors 表）
struct LogMsg {
    level: Level,
    line: String, // JSONL 完整行
}

static INITED: AtomicBool = AtomicBool::new(false);

/// 全局写线程 sender（init 后有效）
static SENDER: Mutex<Option<Sender<LogMsg>>> = Mutex::new(None);

/// 日志根目录（DB 同目录 logs/）
static LOG_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// 初始化（幂等——重复调用忽略）。dir = 日志根目录（一般传 DB 所在目录）。
/// 级别：RUST_LOG 环境变量（默认 info）。
pub fn init(dir: &std::path::Path) {
    if INITED.swap(true, Ordering::SeqCst) {
        return;
    }
    let log_dir = dir.join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    *LOG_DIR.lock().unwrap() = Some(log_dir.clone());

    let (tx, rx) = mpsc::channel::<LogMsg>();
    *SENDER.lock().unwrap() = Some(tx);

    // 专职写线程——文件 append + 过期清理（15 天——Mr2109）
    std::thread::Builder::new()
        .name("rt-logger".into())
        .spawn(move || {
            cleanup_old_logs(&log_dir, 15);
            let mut cur_day = String::new();
            let mut file: Option<File> = None;
            for msg in rx {
                let day = local_today();
                if day != cur_day || file.is_none() {
                    // 按天滚动
                    let path = log_dir.join(format!("roundtable.{day}.log"));
                    file = OpenOptions::new().create(true).append(true).open(path).ok();
                    cur_day = day;
                }
                if let Some(f) = file.as_mut() {
                    let _ = writeln!(f, "{}", msg.line);
                }
            }
        })
        .ok();

    // 挂 log crate（级别 RUST_LOG 默认 info）
    let level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|s| s.parse::<LevelFilter>().ok())
        .unwrap_or(LevelFilter::Info);
    let _ = log::set_boxed_logger(Box::new(RtLogger));
    log::set_max_level(level);
}

/// 本地日期 YYYYMMDD
fn local_today() -> String {
    let now = Local::now();
    format!("{:04}{:02}{:02}", now.year(), now.month(), now.day())
}

/// 清理过期日志（保留天数——启动时一次）
fn cleanup_old_logs(dir: &std::path::Path, keep_days: u32) {
    let cutoff = (Local::now() - chrono::Duration::days(keep_days as i64))
        .format("%Y%m%d")
        .to_string();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            // roundtable.YYYYMMDD.log
            if let Some(day) = name
                .strip_prefix("roundtable.")
                .and_then(|s| s.strip_suffix(".log"))
            {
                if day < cutoff.as_str() {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
}

/// log::Log 实现——格式化 JSONL + send 写线程
struct RtLogger;

impl Log for RtLogger {
    fn enabled(&self, meta: &Metadata) -> bool {
        meta.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{{\"ts\":\"{}\",\"level\":\"{}\",\"module\":\"{}\",\"msg\":{}}}",
            Local::now().format("%Y-%m-%dT%H:%M:%S%:z"),
            record.level(),
            module_path_short(record.module_path()),
            json_escape(&record.args().to_string()),
        );
        if let Some(tx) = SENDER.lock().unwrap().as_ref() {
            let _ = tx.send(LogMsg {
                level: record.level(),
                line,
            });
        }
        // WARN/ERROR 同步镜像 stderr（终端可见——不受写线程时序影响）
        if record.level() <= Level::Warn {
            eprintln!(
                "[{}] {}: {}",
                record.level(),
                module_path_short(record.module_path()),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

/// module_path 缩短（去 crate 前缀——zerg_roundtable::engine::run → engine::run）
fn module_path_short(p: Option<&str>) -> String {
    p.unwrap_or("app")
        .split_once("::")
        .map(|(_, rest)| rest.to_string())
        .unwrap_or_else(|| p.unwrap_or("app").to_string())
}

/// JSON 字符串转义（最小实现——引号/反斜杠/控制符）
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_escape() {
        assert_eq!(json_escape("he said \"hi\""), "\"he said \\\"hi\\\"\"");
        assert_eq!(json_escape("line1\nline2"), "\"line1\\nline2\"");
        assert_eq!(json_escape("普通中文"), "\"普通中文\"");
    }

    #[test]
    fn test_module_path_short() {
        assert_eq!(
            module_path_short(Some("zerg_roundtable::engine::run")),
            "engine::run"
        );
        assert_eq!(module_path_short(None), "app");
    }

    #[test]
    fn test_init_idempotent() {
        let dir = std::env::temp_dir().join(format!("rt_log_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        init(&dir);
        init(&dir); // 二次幂等
        log::info!("测试日志行");
        std::thread::sleep(std::time::Duration::from_millis(100)); // 等写线程
        let today = local_today();
        let path = dir.join("logs").join(format!("roundtable.{today}.log"));
        assert!(path.exists(), "日志文件应存在: {}", path.display());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("测试日志行"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
