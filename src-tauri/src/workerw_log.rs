//! Log de diagnóstico para ficheiro (crash / wallpaper) — `stdout` pode não fazer flush antes da morte do processo.
//! Caminho por defeito: `%LOCALAPPDATA%\\com.calendario.widget\\logs\\workerw.log`
//! Variáveis: `AGENDA_WORKERW_LOG` (caminho completo), `AGENDA_WORKERW_FILE_LOG=0` para desligar.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

fn file_logging_enabled() -> bool {
    std::env::var("AGENDA_WORKERW_FILE_LOG")
        .ok()
        .as_deref()
        != Some("0")
}

fn default_log_path() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("com.calendario.widget")
        .join("logs")
        .join("workerw.log")
}

fn resolved_path() -> PathBuf {
    std::env::var("AGENDA_WORKERW_LOG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_log_path())
}

/// Append uma linha com timestamp local e flush imediato.
pub fn append_line(msg: &str) {
    if !file_logging_enabled() {
        return;
    }
    let path = resolved_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("{ts} [agenda] workerw_file {msg}\n");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
    }
}
