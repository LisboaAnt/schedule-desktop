//! Modelo unificado de evento (UI ↔ Rust ↔ Google Calendar API v3).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CalendarAttendee {
    pub email: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// Estado do formulário / metadados extra vindos da API (guardados em `extras_json` na cache).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEventForm {
    #[serde(default)]
    pub hangout_link: Option<String>,
    #[serde(default)]
    pub recurrence: Option<Vec<String>>,
    #[serde(default)]
    pub reminders_use_default: Option<bool>,
    #[serde(default)]
    pub reminder_popup_minutes: Option<i32>,
    #[serde(default)]
    pub attendees: Option<Vec<CalendarAttendee>>,
    #[serde(default)]
    pub transparency: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub color_id: Option<String>,
    #[serde(default)]
    pub guests_can_modify: Option<bool>,
    #[serde(default)]
    pub guests_can_invite_others: Option<bool>,
    #[serde(default)]
    pub guests_can_see_other_guests: Option<bool>,
}

/// Campos extra enviados pelo cliente em criar/atualizar evento.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EventWriteExtensions {
    pub request_google_meet: bool,
    /// `none` | `daily` | `weekly` | `monthly` | `yearly`
    pub recurrence: String,
    pub use_default_reminders: bool,
    pub reminder_minutes: Option<u32>,
    /// `opaque` (ocupado) | `transparent` (livre)
    pub transparency: String,
    /// `default` | `public` | `private` | `confidential`
    pub visibility: String,
    pub color_id: Option<String>,
    pub attendees: Vec<String>,
    pub guests_can_modify: bool,
    pub guests_can_invite_others: bool,
    pub guests_can_see_other_guests: bool,
}

impl Default for EventWriteExtensions {
    fn default() -> Self {
        Self {
            request_google_meet: false,
            recurrence: "none".to_string(),
            use_default_reminders: true,
            reminder_minutes: Some(30),
            transparency: "opaque".to_string(),
            visibility: "default".to_string(),
            color_id: None,
            attendees: Vec::new(),
            guests_can_modify: false,
            guests_can_invite_others: true,
            guests_can_see_other_guests: true,
        }
    }
}

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
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub form: Option<CalendarEventForm>,
}
