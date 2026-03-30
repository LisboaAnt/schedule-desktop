//! Base de dados local (cache de eventos + estado de sync). Fase 2 — Google Calendar.
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{Connection, OptionalExtension};
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

fn migrate_cached_events_extra_columns(conn: &Connection) -> rusqlite::Result<()> {
    let mut cols: Vec<String> = conn
        .prepare("PRAGMA table_info(cached_events)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<_, _>>()?;
    if !cols.iter().any(|c| c == "description") {
        conn.execute(
            "ALTER TABLE cached_events ADD COLUMN description TEXT",
            [],
        )?;
        cols.push("description".into());
    }
    if !cols.iter().any(|c| c == "location") {
        conn.execute("ALTER TABLE cached_events ADD COLUMN location TEXT", [])?;
    }
    if !cols.iter().any(|c| c == "extras_json") {
        conn.execute(
            "ALTER TABLE cached_events ADD COLUMN extras_json TEXT",
            [],
        )?;
    }
    Ok(())
}

fn form_json(ev: &CalendarEvent) -> Option<String> {
    use crate::calendar_model::CalendarEventForm;
    ev.form.as_ref().and_then(|f| {
        if f == &CalendarEventForm::default() {
            None
        } else {
            serde_json::to_string(f).ok()
        }
    })
}

/// Cria o ficheiro SQLite e tabelas. Idempotente.
pub fn init(app: &AppHandle) -> Result<(), String> {
    let path = db_path(app)?;
    let conn = Connection::open(&path).map_err(|e| format!("SQLite open: {e}"))?;
    init_schema(&conn).map_err(|e| format!("SQLite schema: {e}"))?;
    migrate_cached_events_extra_columns(&conn).map_err(|e| format!("SQLite migrate: {e}"))?;
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
            r#"INSERT INTO cached_events (id, calendar_id, summary, start_at, end_at, description, location, extras_json, raw_json, updated_at_ms)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9)"#,
            rusqlite::params![
                &ev.id,
                &ev.calendar_id,
                &ev.summary,
                &ev.start_at,
                &ev.end_at,
                &ev.description,
                &ev.location,
                form_json(ev),
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
    conn.execute(
        "DELETE FROM sync_state WHERE calendar_id = ?1",
        [calendar_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_sync_token(app: &AppHandle, calendar_id: &str) -> Result<Option<String>, String> {
    let path = db_path(app)?;
    let conn = Connection::open(&path).map_err(|e| format!("SQLite open: {e}"))?;
    let col: Option<Option<String>> = conn
        .query_row(
            "SELECT sync_token FROM sync_state WHERE calendar_id = ?1",
            [calendar_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(col.flatten().filter(|s| !s.is_empty()))
}

pub fn set_sync_state(
    app: &AppHandle,
    calendar_id: &str,
    sync_token: Option<&str>,
    last_sync_ms: i64,
) -> Result<(), String> {
    let path = db_path(app)?;
    let conn = Connection::open(&path).map_err(|e| format!("SQLite open: {e}"))?;
    conn.execute(
        r#"INSERT INTO sync_state (calendar_id, sync_token, last_sync_ms)
           VALUES (?1, ?2, ?3)
           ON CONFLICT(calendar_id) DO UPDATE SET
             sync_token = excluded.sync_token,
             last_sync_ms = excluded.last_sync_ms"#,
        rusqlite::params![calendar_id, sync_token, last_sync_ms],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Remove só a linha de sync (ex.: token expirado, antes de sync completa).
pub fn clear_sync_state_row(app: &AppHandle, calendar_id: &str) -> Result<(), String> {
    let path = db_path(app)?;
    let conn = Connection::open(&path).map_err(|e| format!("SQLite open: {e}"))?;
    conn.execute(
        "DELETE FROM sync_state WHERE calendar_id = ?1",
        [calendar_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn upsert_cached_event(app: &AppHandle, ev: &CalendarEvent) -> Result<(), String> {
    let path = db_path(app)?;
    let conn = Connection::open(&path).map_err(|e| format!("SQLite open: {e}"))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    conn.execute(
        r#"INSERT INTO cached_events (id, calendar_id, summary, start_at, end_at, description, location, extras_json, raw_json, updated_at_ms)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9)
           ON CONFLICT(id, calendar_id) DO UPDATE SET
             summary = excluded.summary,
             start_at = excluded.start_at,
             end_at = excluded.end_at,
             description = excluded.description,
             location = excluded.location,
             extras_json = excluded.extras_json,
             updated_at_ms = excluded.updated_at_ms"#,
        rusqlite::params![
            &ev.id,
            &ev.calendar_id,
            &ev.summary,
            &ev.start_at,
            &ev.end_at,
            &ev.description,
            &ev.location,
            form_json(ev),
            now,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_cached_event(app: &AppHandle, calendar_id: &str, event_id: &str) -> Result<(), String> {
    let path = db_path(app)?;
    let conn = Connection::open(&path).map_err(|e| format!("SQLite open: {e}"))?;
    conn.execute(
        "DELETE FROM cached_events WHERE calendar_id = ?1 AND id = ?2",
        rusqlite::params![calendar_id, event_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn list_cached_events(app: &AppHandle) -> Result<Vec<CalendarEvent>, String> {
    let path = db_path(app)?;
    let conn = Connection::open(&path).map_err(|e| format!("SQLite open: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT id, calendar_id, summary, start_at, end_at, description, location, extras_json FROM cached_events ORDER BY start_at",
        )
        .map_err(|e| e.to_string())?;
    let iter = stmt
        .query_map([], |row| {
            let form_str: Option<String> = row.get(7)?;
            let form = form_str.and_then(|s| serde_json::from_str(&s).ok());
            Ok(CalendarEvent {
                id: row.get(0)?,
                calendar_id: row.get(1)?,
                summary: row.get(2)?,
                start_at: row.get(3)?,
                end_at: row.get(4)?,
                description: row.get(5)?,
                location: row.get(6)?,
                form,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in iter {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}
