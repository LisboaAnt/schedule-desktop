//! Modelo unificado de evento (UI ↔ Rust ↔ Google Calendar API v3 na Fase 2).
//! Por agora só tipos; a UI continua a usar dados de demonstração em `agenda.js`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub id: String,
    pub calendar_id: String,
    pub summary: String,
    #[serde(default)]
    pub start_at: Option<String>,
    #[serde(default)]
    pub end_at: Option<String>,
}
