//! 诊断日志：把网络/时序诊断**同时**写到 stderr 与磁盘文件（带毫秒时间戳）。
//!
//! 目的：联机延迟/卡顿问题需要把 host 与 client 两端的时序对齐分析。此前只能靠人工把控制台
//! 内容复制到 txt；改为进程内直接落盘后，双方各跑一局即可把 `logs/` 下的文件拿来分析。
//!
//! 约定：诊断用 `logging::log(&format!(...))`（不要用裸 `eprintln!`），这样两端都有统一时间戳。
//! 文件名：`logs/<role>-<epoch秒>.log`（role = host/client/app）。
//! 注：`net-steam` 等库内的 `eprintln!` 不经过这里；用 `run-steam.ps1` 的 tee 兜底捕获。

use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static FILE: OnceLock<Mutex<std::fs::File>> = OnceLock::new();
static ROLE: OnceLock<String> = OnceLock::new();

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// 初始化诊断日志（进程启动时调用一次）。失败时静默降级为“只 stderr”。
pub fn init(role: &str) {
    let _ = ROLE.set(role.to_string());
    let _ = std::fs::create_dir_all("logs");
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = format!("logs/{role}-{secs}.log");
    match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(f) => {
            let _ = FILE.set(Mutex::new(f));
            log(&format!("diag logging -> {path}"));
        }
        Err(e) => eprintln!("[diag] cannot open {path}: {e}"),
    }
}

/// 写一行带毫秒时间戳的诊断（stderr + 文件）。
pub fn log(msg: &str) {
    let ms = now_ms();
    let role = ROLE.get().map(|s| s.as_str()).unwrap_or("app");
    let line = format!("{ms} [{role}] {msg}");
    // 先落盘再 stderr：即使控制台被关/重定向丢失，文件仍完整。
    if let Some(f) = FILE.get() {
        if let Ok(mut f) = f.lock() {
            let _ = writeln!(f, "{line}");
        }
    }
    eprintln!("{line}");
}
