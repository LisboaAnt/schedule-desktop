//! Base de dados local (cache de eventos + estado de sync). Fase 2 — Google Calendar.
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::Connection;
use tauri::{AppHandle, Manager};

use crate::calendar_model::CalendarEvent;

static INIT_OK: AtomicBool = AtomicBool::new(false);

fn db_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("app_local_data_dir: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("create_dir_all: {e}"))?;
    Ok(dir.join("agenda_cache.sqlite3"))
}

fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS cached_events (
            id TEXT NOT NULL,
            calendar_id TEXT NOT NULL,
            summary TEXT NOT NULL DEFAULT '',
            start_at TEXT,
            end_at TEXT,
            raw_json TEXT,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY (id, calendar_id)
        );
        CREATE INDEX IF NOT EXISTS idx_cached_events_calendar ON cached_events(calendar_id);
        CREATE INDEX IF NOT EXISTS idx_cached_events_start ON cached_events(start_at);
        CREATE TABLE IF NOT EXISTS sync_state (
            calendar_id TEXT PRIMARY KEY,
            sync_token TEXT,
            last_sync_ms INTEGER
        );
        "#,
    )?;
    Ok(())
}

/// Cria o ficheiro SQLite e tabelas. Idempotente.
pub fn init(app: &AppHandle) -> Result<(), String> {
    let path = db_path(app)?;
    let conn = Connection::open(&path).map_err(|e| format!("SQLite open: {e}"))?;
    init_schema(&conn).map_err(|e| format!("SQLite schema: {e}"))?;
    INIT_OK.store(true, Ordering::SeqCst);
    Ok(())
}

pub fn is_ready() -> bool {
    INIT_OK.load(Ordering::SeqCst)
}

/// Substitui todos os eventos em cache desse calendário (sync completo da janela pedida).
pub fn replace_calendar_events(
    app: &AppHandle,
    calendar_id: &str,
    events: &[CalendarEvent],
) -> Result<(), String> {
    let path = db_path(app)?;
    let mut conn = Connection::open(&path).map_err(|e| format!("SQLite open: {e}"))?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM cached_events WHERE calendar_id = ?1",
        [calendar_id],
    )
    .map_err(|e| e.to_string())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    for ev in events {
        tx.execute(
            r#"INSERT INTO cached_events (id, calendar_id, summary, start_at, end_at, raw_json, updated_at_ms)
               VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)"#,
            rusqlite::params![
                &ev.id,
                &ev.calendar_id,
                &ev.summary,
                &ev.start_at,
                &ev.end_at,
                now,
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn clear_calendar_events(app: &AppHandle, calendar_id: &str) -> Result<(), String> {
    let path = db_path(app)?;
    let conn = Connection::open(&path).map_err(|e| format!("SQLite open: {e}"))?;
    conn.execute(
        "DELETE FROM cached_events WHERE calendar_id = ?1",
        [calendar_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn list_cached_events(app: &AppHandle) -> Result<Vec<CalendarEvent>, String> {
    let path = db_path(app)?;
    let conn = Connection::open(&path).map_err(|e| format!("SQLite open: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT id, calendar_id, summary, start_at, end_at FROM cached_events ORDER BY start_at",
        )
        .map_err(|e| e.to_string())?;
    let iter = stmt
        .query_map([], |row| {
            Ok(CalendarEvent {
                id: row.get(0)?,
                calendar_id: row.get(1)?,
                summary: row.get(2)?,
                start_at: row.get(3)?,
                end_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in iter {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}
