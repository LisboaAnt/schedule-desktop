//! Vigia mínimo (Windows): lança o executável principal, espera que termine e volta a lançar se
//! o código de saída indicar falha, **ou** (excepção) saída limpa com `desktopBehindIcons` em
//! `config.json` **e** sem ficheiro `user_quit_watchdog.flag` (fecho tipo mudança de wallpaper).
//!
//! Variáveis de ambiente:
//! - `AGENDA_CHILD_EXE` — caminho absoluto do `.exe` principal (opcional se `--child` ou procura ao lado).
//! - `AGENDA_WATCHDOG_MAX_ATTEMPTS` — tentativas de arranque por sessão do vigia (defeito 5).
//! - `AGENDA_WATCHDOG_BACKOFF_MS` — backoff inicial em ms (defeito 2000).
//! - `AGENDA_WATCHDOG_BACKOFF_CAP_MS` — tecto do backoff (defeito 60000).
//! - `AGENDA_WATCHDOG_PRE_RETRY_DELAY_MS` — atraso após saída com falha, antes do backoff (defeito 0); sobrepõe o valor em `config.json` se definido.
//! - `watchdogPreRetryDelayMs` em `%APPDATA%\\com.calendario.widget\\config.json` (ou `%LOCALAPPDATA%` como recurso) — gravado pela app em Definições.
//! - `AGENDA_WATCHDOG_RELUNCH_ON_ZERO=1` — trata saída com código 0 como falha e relança (perigoso; ver docs).
//! - `AGENDA_WATCHDOG_LOG=0` — desliga o log em ficheiro.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::thread;
use std::time::Duration;

use serde::Deserialize;

/// Só os campos necessários; o resto do `config.json` da app ignora-se.
#[derive(Debug, Deserialize)]
struct ConfigWatchdogSlice {
    #[serde(default, rename = "watchdogPreRetryDelayMs")]
    watchdog_pre_retry_delay_ms: Option<u64>,
    /// Modo «atrás dos ícones» — se `true` e saída limpa sem ficheiro de «Sair», relança (wallpaper, etc.).
    #[serde(default, rename = "desktopBehindIcons")]
    desktop_behind_icons: Option<bool>,
}

fn file_log_enabled() -> bool {
    std::env::var("AGENDA_WATCHDOG_LOG").ok().as_deref() != Some("0")
}

fn default_log_path() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("com.calendario.widget")
        .join("logs")
        .join("watchdog.log")
}

fn append_log_line(msg: &str) {
    if !file_log_enabled() {
        return;
    }
    let path = default_log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("{ts} [agenda] watchdog {msg}\n");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
    }
}

fn parse_args() -> (Option<PathBuf>, bool) {
    let mut args = std::env::args_os().skip(1);
    let mut child: Option<PathBuf> = None;
    let mut show_help = false;
    while let Some(a) = args.next() {
        if a == "--help" || a == "-h" {
            show_help = true;
            continue;
        }
        if a == "--child" {
            if let Some(p) = args.next() {
                child = Some(PathBuf::from(p));
            }
            continue;
        }
        if child.is_none() && !a.to_string_lossy().starts_with('-') {
            child = Some(PathBuf::from(a));
        }
    }
    (child, show_help)
}

fn resolve_child_exe(cli: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(p) = cli {
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!("Caminho inválido ou ficheiro em falta: {}", p.display()));
    }
    if let Ok(s) = std::env::var("AGENDA_CHILD_EXE") {
        let p = PathBuf::from(s.trim());
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!("AGENDA_CHILD_EXE não aponta para um ficheiro: {}", p.display()));
    }
    let self_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = self_exe
        .parent()
        .ok_or_else(|| "Sem pasta do executável do vigia.".to_string())?;
    for name in ["Agenda.exe", "calendario-app.exe"] {
        let p = dir.join(name);
        if p.is_file() {
            append_log_line(&format!("child_resolved name={name} path={}", p.display()));
            return Ok(p);
        }
    }
    Err(
        "Não foi encontrado Agenda.exe nem calendario-app.exe na mesma pasta que o vigia. \
         Usa --child ou AGENDA_CHILD_EXE."
            .to_string(),
    )
}

fn max_attempts() -> u32 {
    std::env::var("AGENDA_WATCHDOG_MAX_ATTEMPTS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| (1..=50).contains(&n))
        .unwrap_or(5)
}

fn backoff_initial_ms() -> u64 {
    std::env::var("AGENDA_WATCHDOG_BACKOFF_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| (100..=120_000).contains(&n))
        .unwrap_or(2000)
}

fn backoff_cap_ms() -> u64 {
    std::env::var("AGENDA_WATCHDOG_BACKOFF_CAP_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| (500..=600_000).contains(&n))
        .unwrap_or(60_000)
}

fn app_config_json_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    // Tauri `app_config_dir` no Windows: normalmente %APPDATA%\identifier
    if let Ok(app) = std::env::var("APPDATA") {
        v.push(
            PathBuf::from(app)
                .join("com.calendario.widget")
                .join("config.json"),
        );
    }
    if let Ok(loc) = std::env::var("LOCALAPPDATA") {
        v.push(
            PathBuf::from(loc)
                .join("com.calendario.widget")
                .join("config.json"),
        );
    }
    v
}

fn user_quit_flag_paths() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(app) = std::env::var("APPDATA") {
        v.push(
            PathBuf::from(app)
                .join("com.calendario.widget")
                .join("user_quit_watchdog.flag"),
        );
    }
    if let Ok(loc) = std::env::var("LOCALAPPDATA") {
        v.push(
            PathBuf::from(loc)
                .join("com.calendario.widget")
                .join("user_quit_watchdog.flag"),
        );
    }
    v
}

fn user_quit_flag_exists() -> bool {
    user_quit_flag_paths().iter().any(|p| p.is_file())
}

fn remove_user_quit_flags() {
    for p in user_quit_flag_paths() {
        let _ = std::fs::remove_file(p);
    }
}

fn desktop_behind_icons_from_config_file() -> bool {
    for path in app_config_json_candidates() {
        if !path.is_file() {
            continue;
        }
        let Ok(s) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(cfg) = serde_json::from_str::<ConfigWatchdogSlice>(&s) else {
            continue;
        };
        return cfg.desktop_behind_icons.unwrap_or(false);
    }
    false
}

fn pre_retry_delay_ms_from_config_file() -> Option<u64> {
    for path in app_config_json_candidates() {
        if !path.is_file() {
            continue;
        }
        let Ok(s) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(cfg) = serde_json::from_str::<ConfigWatchdogSlice>(&s) else {
            continue;
        };
        if let Some(ms) = cfg.watchdog_pre_retry_delay_ms {
            let ms = ms.min(10_000);
            append_log_line(&format!(
                "config_watchdog_pre_retry_delay_ms={ms} path={}",
                path.display()
            ));
            return Some(ms);
        }
    }
    None
}

/// Atraso após o processo filho terminar com **falha** (ou com 0 se `RELUNCH_ON_ZERO`), antes do backoff.
/// Ordem: variável de ambiente (testes) > `config.json` (Definições na app) > 0.
fn pre_retry_delay_ms() -> u64 {
    if let Ok(v) = std::env::var("AGENDA_WATCHDOG_PRE_RETRY_DELAY_MS") {
        if let Ok(n) = v.parse::<u64>() {
            if n <= 10_000 {
                return n;
            }
        }
    }
    pre_retry_delay_ms_from_config_file().unwrap_or(0)
}

fn relaunch_on_clean_exit() -> bool {
    std::env::var("AGENDA_WATCHDOG_RELUNCH_ON_ZERO")
        .ok()
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn run() -> Result<ExitCode, String> {
    let (cli_child, help) = parse_args();
    if help {
        eprintln!(
            "Uso: agenda-watchdog.exe [--child caminho\\para\\Agenda.exe]\n\
             Ou defina AGENDA_CHILD_EXE. Procura Agenda.exe / calendario-app.exe na mesma pasta.\n\
             Variáveis: AGENDA_WATCHDOG_PRE_RETRY_DELAY_MS, AGENDA_WATCHDOG_RELUNCH_ON_ZERO (ver docs/WATCHDOG.md)."
        );
        return Ok(ExitCode::SUCCESS);
    }

    let child_path = resolve_child_exe(cli_child)?;
    append_log_line(&format!("session_start child={}", child_path.display()));

    if relaunch_on_clean_exit() {
        append_log_line("warning relaunch_on_zero=1 (saída 0 será relançada; «Sair» na bandeja também)");
        eprintln!(
            "[agenda-watchdog] AGENDA_WATCHDOG_RELUNCH_ON_ZERO=1: saídas com código 0 voltam a lançar a app até ao máximo de tentativas."
        );
    }

    let max = max_attempts();
    let mut backoff_ms = backoff_initial_ms();
    let cap_ms = backoff_cap_ms();

    for attempt in 1..=max {
        append_log_line(&format!("spawn attempt={attempt}/{max}"));
        let mut cmd = Command::new(&child_path);
        cmd.env(
            "AGENDA_WATCHDOG_SESSION",
            format!("{attempt}/{max}"),
        );
        let mut child = cmd.spawn().map_err(|e| {
            format!(
                "Falha a lançar {}: {e}",
                child_path.display()
            )
        })?;

        let status = child.wait().map_err(|e| format!("wait: {e}"))?;
        let code = status.code();
        let success = status.success();
        append_log_line(&format!(
            "child_exit attempt={attempt} success={success} code={code:?}"
        ));

        if success && !relaunch_on_clean_exit() {
            if user_quit_flag_exists() {
                remove_user_quit_flags();
                append_log_line("session_end clean_exit user_quit_flag");
                return Ok(ExitCode::SUCCESS);
            }
            if !desktop_behind_icons_from_config_file() {
                append_log_line("session_end clean_exit");
                return Ok(ExitCode::SUCCESS);
            }
            append_log_line(
                "clean_exit with desktopBehindIcons: retrying (fecho inesperado ex. wallpaper)",
            );
            // Continua como falha: backoff + nova tentativa (até max_attempts).
        }

        if success && relaunch_on_clean_exit() {
            append_log_line("clean_exit_relaunch_on_zero retry_as_failure");
        }

        if attempt >= max {
            append_log_line("session_end max_attempts_abort");
            return Ok(ExitCode::from(1));
        }

        let pre = pre_retry_delay_ms();
        if pre > 0 {
            append_log_line(&format!("pre_retry_delay_ms={pre}"));
            thread::sleep(Duration::from_millis(pre));
        }

        thread::sleep(Duration::from_millis(backoff_ms));
        backoff_ms = (backoff_ms.saturating_mul(2)).min(cap_ms);
    }

    unreachable!()
}

fn main() -> ExitCode {
    match run() {
        Ok(c) => c,
        Err(e) => {
            append_log_line(&format!("fatal_err={e}"));
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
