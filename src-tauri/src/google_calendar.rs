//! OAuth 2.0 Google (PKCE + localhost) e Calendar API v3 (cache SQLite).
//! Client ID (primeiro que existir): variável de ambiente em runtime `GOOGLE_OAUTH_CLIENT_ID`,
//! ou em **compile time** `GOOGLE_OAUTH_CLIENT_ID` ao correr `cargo`/`tauri build`, ou ficheiro
//! `google_oauth_client_id.txt` em `app_config_dir`.
//! **Client secret** (`GOOGLE_OAUTH_CLIENT_SECRET` ou `google_oauth_client_secret.txt`): só se o cliente
//! OAuth na Google estiver como **aplicação Web**; o tipo **Desktop** funciona só com PKCE (sem secret).
//! Redirect fixo: `http://127.0.0.1:17892/callback` — adiciona este URI nas credenciais OAuth (app desktop).

use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::{DateTime, Datelike, Months, NaiveDate, Utc, Weekday};
use keyring::Entry;
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value};
use urlencoding::encode as urlenc;
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use tauri::Manager;

use crate::calendar_model::{
    CalendarAttendee, CalendarEvent, CalendarEventForm, EventWriteExtensions,
};
use crate::local_store;

const KEYRING_SERVICE: &str = "calendario-app";
const KEYRING_USER_REFRESH: &str = "google_oauth_refresh_token";
/// Ficheiro na pasta de dados local (ao lado do SQLite). O keyring no Windows em dev falha por vezes; o ficheiro garante persistência.
const REFRESH_TOKEN_FILENAME: &str = "google_oauth_refresh_token";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
/// Leitura + criação/edição de eventos. Após mudar o escopo, volta a **Ligar conta Google**.
const SCOPE: &str = "https://www.googleapis.com/auth/calendar.events";
const PRIMARY: &str = "primary";
/// Porta do servidor local de callback OAuth (deve coincidir com o URI na Google Cloud Console).
const OAUTH_CALLBACK_PORT: u16 = 17_892;
/// Evita dois `bind` na mesma porta (erro 10048 no Windows) e mensagens confusas.
static OAUTH_FLOW_MUTEX: Mutex<()> = Mutex::new(());
/// Se o utilizador não voltar do browser, libertamos a porta após este tempo.
const OAUTH_CALLBACK_WAIT_SECS: u64 = 600;

#[derive(Debug, Deserialize)]
struct TokenJson {
    access_token: String,
    refresh_token: Option<String>,
    #[allow(dead_code)]
    expires_in: Option<i64>,
}

fn oauth_client_id_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("app_config_dir: {e}"))?;
    Ok(dir.join("google_oauth_client_id.txt"))
}

/// Resolve o ID de cliente OAuth (nunca o secret em disco versionado).
///
/// Ordem: env em runtime → valor definido na **compilação** (`GOOGLE_OAUTH_CLIENT_ID` ao fazer build) → ficheiro local.
/// Para distribuir a todos sem ficheiros extra: define `GOOGLE_OAUTH_CLIENT_ID` no ambiente ao gerar o instalador
/// (o ID fica no binário; não precisas de `client_secret` com PKCE).
pub fn resolve_client_id(app: &AppHandle) -> Result<String, String> {
    if let Ok(v) = std::env::var("GOOGLE_OAUTH_CLIENT_ID") {
        let t = v.trim().to_string();
        if !t.is_empty() {
            return Ok(t);
        }
    }
    if let Some(v) = option_env!("GOOGLE_OAUTH_CLIENT_ID") {
        let t = v.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    let path = oauth_client_id_path(app)?;
    if path.exists() {
        let s = std::fs::read_to_string(&path).map_err(|e| format!("ler client id: {e}"))?;
        let t = s.trim().to_string();
        if !t.is_empty() {
            return Ok(t);
        }
    }
    Err(
        "OAuth: define GOOGLE_OAUTH_CLIENT_ID (runtime ou na build), ou google_oauth_client_id.txt na pasta de configuração da app."
            .into(),
    )
}

pub fn client_id_configured(app: &AppHandle) -> bool {
    resolve_client_id(app).is_ok()
}

fn oauth_client_secret_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("app_config_dir: {e}"))?;
    Ok(dir.join("google_oauth_client_secret.txt"))
}

/// Secreto OAuth: necessário se o ID de cliente for tipo **Web** na Google Cloud; omitir para tipo **Desktop**.
pub fn resolve_client_secret(app: &AppHandle) -> Option<String> {
    if let Ok(v) = std::env::var("GOOGLE_OAUTH_CLIENT_SECRET") {
        let t = v.trim().to_string();
        if !t.is_empty() {
            return Some(t);
        }
    }
    if let Some(v) = option_env!("GOOGLE_OAUTH_CLIENT_SECRET") {
        let t = v.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    if let Ok(path) = oauth_client_secret_path(app) {
        if path.exists() {
            if let Ok(s) = std::fs::read_to_string(&path) {
                let t = s.trim().to_string();
                if !t.is_empty() {
                    return Some(t);
                }
            }
        }
    }
    None
}

fn keyring_entry() -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, KEYRING_USER_REFRESH).map_err(|e| e.to_string())
}

fn refresh_token_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("app_local_data_dir: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("create_dir_all: {e}"))?;
    Ok(dir.join(REFRESH_TOKEN_FILENAME))
}

fn read_refresh_token_file(app: &AppHandle) -> Option<String> {
    refresh_token_file_path(app).ok().and_then(|p| {
        fs::read_to_string(&p)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    })
}

fn write_refresh_token_file(app: &AppHandle, token: &str) -> Result<(), String> {
    let p = refresh_token_file_path(app)?;
    fs::write(&p, token).map_err(|e| format!("Não foi possível guardar o token OAuth: {e}"))
}

fn remove_refresh_token_file(app: &AppHandle) {
    if let Ok(p) = refresh_token_file_path(app) {
        let _ = fs::remove_file(p);
    }
}

pub fn has_refresh_token(app: &AppHandle) -> bool {
    read_refresh_token_file(app).is_some()
        || keyring_entry()
            .ok()
            .and_then(|e| e.get_password().ok())
            .filter(|s| !s.is_empty())
            .is_some()
}

fn store_refresh_token(app: &AppHandle, token: &str) -> Result<(), String> {
    write_refresh_token_file(app, token)?;
    if let Ok(e) = keyring_entry() {
        let _ = e.set_password(token);
    }
    Ok(())
}

pub fn delete_refresh_token(app: &AppHandle) -> Result<(), String> {
    remove_refresh_token_file(app);
    if let Ok(e) = keyring_entry() {
        let _ = e.delete_credential();
    }
    Ok(())
}

fn get_refresh_token(app: &AppHandle) -> Result<String, String> {
    if let Some(t) = read_refresh_token_file(app) {
        return Ok(t);
    }
    let e = keyring_entry()?;
    let t = e
        .get_password()
        .map_err(|e| format!("Token OAuth em falta: {e}. Volta a ligar a conta Google."))?;
    if t.trim().is_empty() {
        return Err("Token OAuth em falta. Volta a ligar a conta Google.".into());
    }
    let _ = write_refresh_token_file(app, t.trim());
    Ok(t.trim().to_string())
}

fn pkce_verifier() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut rng = rand::thread_rng();
    (0..64)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

fn pkce_challenge_s256(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

fn random_state() -> String {
    let mut rng = rand::thread_rng();
    let b: [u8; 16] = rng.gen();
    hex_lower(&b)
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

/// Aceita uma ligação TCP no listener com tempo limite (evita `accept` infinito e porta presa).
fn wait_oauth_tcp_connection(listener: &TcpListener) -> Result<TcpStream, String> {
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("OAuth: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(OAUTH_CALLBACK_WAIT_SECS);
    loop {
        match listener.accept() {
            Ok((stream, _)) => return Ok(stream),
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(
                        "OAuth: tempo esgotado à espera do browser. Fecha outras instâncias da app, tenta de novo."
                            .into(),
                    );
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => return Err(format!("OAuth: callback: aceitar ligação: {e}")),
        }
    }
}

/// Lê um único GET no callback; devolve `code` e valida `state`.
fn read_oauth_callback(mut stream: TcpStream, expected_state: &str) -> Result<String, String> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(300)));
    let mut buf = [0u8; 16384];
    let n = stream
        .read(&mut buf)
        .map_err(|e| format!("callback: ler pedido: {e}"))?;
    let raw = std::str::from_utf8(&buf[..n]).map_err(|_| "callback: UTF-8 inválido")?;
    let first = raw
        .lines()
        .next()
        .ok_or_else(|| "callback: pedido vazio".to_string())?;
    let mut parts = first.split_whitespace();
    let _method = parts.next().ok_or_else(|| "callback: método em falta".to_string())?;
    let path = parts.next().ok_or_else(|| "callback: path em falta".to_string())?;
    let u = url::Url::parse(&format!("http://127.0.0.1{}", path))
        .map_err(|e| format!("callback: URL: {e}"))?;
    for (k, v) in u.query_pairs() {
        if k == "error" {
            return Err(format!(
                "OAuth recusado: {v} ({})",
                u.query_pairs()
                    .find(|(a, _)| a == "error_description")
                    .map(|(_, d)| d.to_string())
                    .unwrap_or_default()
            ));
        }
    }
    let state_ok = u
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v == expected_state)
        .unwrap_or(false);
    if !state_ok {
        return Err("OAuth: state inválido (possível CSRF).".into());
    }
    let code = u
        .query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.to_string())
        .ok_or_else(|| "OAuth: código em falta na resposta.".to_string())?;

    let body = b"<html><body>Pode fechar esta janela.</body></html>";
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();

    Ok(code)
}

/// Abre o browser, recebe o código e troca por tokens; guarda refresh token no keyring.
pub fn run_desktop_oauth_flow(app: &AppHandle, client_id: &str) -> Result<(), String> {
    let _flow_guard = OAUTH_FLOW_MUTEX.try_lock().map_err(|e| match e {
        std::sync::TryLockError::WouldBlock => {
            "OAuth: já há um login em curso. Completa no browser ou aguarda; não abras dois logins ao mesmo tempo.".to_string()
        }
        std::sync::TryLockError::Poisoned(_) => "OAuth: reinicia a app e tenta de novo.".to_string(),
    })?;

    let listener = TcpListener::bind(("127.0.0.1", OAUTH_CALLBACK_PORT)).map_err(|e| {
        format!(
            "OAuth: não foi possível abrir 127.0.0.1:{} — fecha outras instâncias da app (ou o gestor de tarefas: calendario-app) e tenta de novo. ({e})",
            OAUTH_CALLBACK_PORT
        )
    })?;
    let redirect_uri = format!("http://127.0.0.1:{OAUTH_CALLBACK_PORT}/callback");

    let verifier = pkce_verifier();
    let challenge = pkce_challenge_s256(&verifier);
    let state = random_state();

    let mut auth = url::Url::parse(AUTH_URL).map_err(|e| e.to_string())?;
    {
        let mut q = auth.query_pairs_mut();
        q.append_pair("client_id", client_id);
        q.append_pair("redirect_uri", &redirect_uri);
        q.append_pair("response_type", "code");
        q.append_pair("scope", SCOPE);
        q.append_pair("code_challenge", &challenge);
        q.append_pair("code_challenge_method", "S256");
        q.append_pair("state", &state);
        q.append_pair("access_type", "offline");
        q.append_pair("prompt", "consent");
    }

    let url_str = auth.as_str();
    open::that(url_str).map_err(|e| format!("OAuth: abrir browser: {e}"))?;

    let stream = wait_oauth_tcp_connection(&listener)?;
    let code = read_oauth_callback(stream, &state)?;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let token_res = match resolve_client_secret(app) {
        Some(ref sec) => client
            .post(TOKEN_URL)
            .form(&[
                ("client_id", client_id),
                ("code", code.as_str()),
                ("code_verifier", verifier.as_str()),
                ("grant_type", "authorization_code"),
                ("redirect_uri", redirect_uri.as_str()),
                ("client_secret", sec.as_str()),
            ])
            .send(),
        None => client
            .post(TOKEN_URL)
            .form(&[
                ("client_id", client_id),
                ("code", code.as_str()),
                ("code_verifier", verifier.as_str()),
                ("grant_type", "authorization_code"),
                ("redirect_uri", redirect_uri.as_str()),
            ])
            .send(),
    }
    .map_err(|e| format!("OAuth: token HTTP: {e}"))?;

    if !token_res.status().is_success() {
        let t = token_res.text().unwrap_or_default();
        let hint = if t.contains("client_secret") {
            " Cria credencial tipo **Desktop** na Google Cloud (sem secret) ou define GOOGLE_OAUTH_CLIENT_SECRET."
        } else {
            ""
        };
        return Err(format!("OAuth: troca de token falhou: {t}{hint}"));
    }

    let tj: TokenJson = token_res
        .json()
        .map_err(|e| format!("OAuth: JSON token: {e}"))?;

    let refresh = tj
        .refresh_token
        .ok_or_else(|| "OAuth: Google não devolveu refresh_token. Tenta de novo com prompt=consent ou revoga o acesso à app nas definições Google.".to_string())?;

    store_refresh_token(app, &refresh)?;
    Ok(())
}

pub async fn refresh_access_token(app: &AppHandle, refresh: &str) -> Result<String, String> {
    let client_id = resolve_client_id(app)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let token_res = match resolve_client_secret(app) {
        Some(ref sec) => {
            client
                .post(TOKEN_URL)
                .form(&[
                    ("client_id", client_id.as_str()),
                    ("refresh_token", refresh),
                    ("grant_type", "refresh_token"),
                    ("client_secret", sec.as_str()),
                ])
                .send()
                .await
        }
        None => {
            client
                .post(TOKEN_URL)
                .form(&[
                    ("client_id", client_id.as_str()),
                    ("refresh_token", refresh),
                    ("grant_type", "refresh_token"),
                ])
                .send()
                .await
        }
    }
    .map_err(|e| format!("token: {e}"))?;

    if !token_res.status().is_success() {
        let t = token_res.text().await.unwrap_or_default();
        return Err(format!("Renovar access token falhou: {t}"));
    }

    let tj: TokenJson = token_res
        .json()
        .await
        .map_err(|e| format!("JSON token: {e}"))?;

    Ok(tj.access_token)
}

/// Anexa instruções quando a API devolve 403 por escopo (token antigo só de leitura, etc.).
fn calendar_api_scope_hint(body: &str) -> Option<&'static str> {
    let b = body.to_ascii_lowercase();
    if b.contains("insufficient authentication scopes")
        || b.contains("access_token_scope_insufficient")
        || b.contains("insufficientpermissions")
        || b.contains("insufficient permission")
    {
        Some(
            "\n\n→ O login Google não inclui permissão para criar ou alterar eventos (p.ex. sessão antiga só de leitura). \
             Em Definições da app: «Desligar Google» e volta a «Ligar conta Google» para aceitar o escopo completo do calendário.",
        )
    } else {
        None
    }
}

fn format_calendar_write_error(context: &str, body: &str) -> String {
    let mut s = format!("{context}: {body}");
    if let Some(h) = calendar_api_scope_hint(body) {
        s.push_str(h);
    }
    s
}

fn parse_event_times(item: &Value) -> (Option<String>, Option<String>) {
    let start = item.get("start");
    let end = item.get("end");
    let s = start.and_then(|o| {
        o.get("dateTime")
            .and_then(|v| v.as_str())
            .or_else(|| o.get("date").and_then(|v| v.as_str()))
            .map(|x| x.to_string())
    });
    let e = end.and_then(|o| {
        o.get("dateTime")
            .and_then(|v| v.as_str())
            .or_else(|| o.get("date").and_then(|v| v.as_str()))
            .map(|x| x.to_string())
    });
    (s, e)
}

fn meet_request_id() -> String {
    format!("meet-{:x}", rand::random::<u128>())
}

fn naive_date_from_start(start_iso: &str, all_day: bool) -> Option<NaiveDate> {
    let s = start_iso.trim();
    if all_day {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
    } else {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.date_naive())
            .or_else(|| {
                if s.len() >= 10 {
                    NaiveDate::parse_from_str(&s[..10], "%Y-%m-%d").ok()
                } else {
                    None
                }
            })
    }
}

fn weekday_to_byday(w: Weekday) -> &'static str {
    match w {
        Weekday::Mon => "MO",
        Weekday::Tue => "TU",
        Weekday::Wed => "WE",
        Weekday::Thu => "TH",
        Weekday::Fri => "FR",
        Weekday::Sat => "SA",
        Weekday::Sun => "SU",
    }
}

fn recurrence_rules_from_ext(
    ext: &EventWriteExtensions,
    start_iso: &str,
    all_day: bool,
    is_patch: bool,
) -> Option<Value> {
    let rec = ext.recurrence.trim();
    let rec = if rec.is_empty() { "none" } else { rec };
    if rec == "none" {
        return if is_patch {
            Some(json!([]))
        } else {
            None
        };
    }
    let date = naive_date_from_start(start_iso, all_day)?;
    let rule = match rec {
        "daily" => "RRULE:FREQ=DAILY".to_string(),
        "weekly" => format!(
            "RRULE:FREQ=WEEKLY;BYDAY={}",
            weekday_to_byday(date.weekday())
        ),
        "monthly" => format!("RRULE:FREQ=MONTHLY;BYMONTHDAY={}", date.day()),
        "yearly" => format!(
            "RRULE:FREQ=YEARLY;BYMONTH={};BYMONTHDAY={}",
            date.month(),
            date.day()
        ),
        _ => return if is_patch { Some(json!([])) } else { None },
    };
    Some(json!([rule]))
}

fn sanitize_attendees(emails: &[String]) -> Vec<Value> {
    emails
        .iter()
        .map(|e| e.trim().to_lowercase())
        .filter(|e| e.contains('@') && e.contains('.') && e.len() > 5)
        .map(|email| json!({ "email": email }))
        .collect()
}

fn parse_form_from_item(item: &Value) -> Option<CalendarEventForm> {
    let mut f = CalendarEventForm::default();
    if let Some(h) = item.get("hangoutLink").and_then(|v| v.as_str()) {
        if !h.is_empty() {
            f.hangout_link = Some(h.to_string());
        }
    }
    if let Some(arr) = item.get("recurrence").and_then(|v| v.as_array()) {
        let rules: Vec<String> = arr
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect();
        if !rules.is_empty() {
            f.recurrence = Some(rules);
        }
    }
    if let Some(rem) = item.get("reminders") {
        f.reminders_use_default = rem.get("useDefault").and_then(|v| v.as_bool());
        if let Some(over) = rem.get("overrides").and_then(|v| v.as_array()) {
            for o in over {
                if o.get("method").and_then(|v| v.as_str()) == Some("popup") {
                    if let Some(m) = o.get("minutes").and_then(|v| v.as_i64()) {
                        f.reminder_popup_minutes = Some(m as i32);
                        break;
                    }
                }
            }
        }
    }
    if let Some(arr) = item.get("attendees").and_then(|v| v.as_array()) {
        let mut atts = Vec::new();
        for a in arr {
            let email = a.get("email").and_then(|v| v.as_str()).unwrap_or("").trim();
            if email.is_empty() {
                continue;
            }
            atts.push(CalendarAttendee {
                email: email.to_string(),
                display_name: a
                    .get("displayName")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            });
        }
        if !atts.is_empty() {
            f.attendees = Some(atts);
        }
    }
    f.transparency = item
        .get("transparency")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    f.visibility = item
        .get("visibility")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    f.color_id = item
        .get("colorId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    f.guests_can_modify = item
        .get("guestsCanModify")
        .and_then(|v| v.as_bool());
    f.guests_can_invite_others = item
        .get("guestsCanInviteOthers")
        .and_then(|v| v.as_bool());
    f.guests_can_see_other_guests = item
        .get("guestsCanSeeOtherGuests")
        .and_then(|v| v.as_bool());

    if f == CalendarEventForm::default() {
        None
    } else {
        Some(f)
    }
}

fn event_from_api_item(item: &Value) -> Option<CalendarEvent> {
    let id = item.get("id")?.as_str()?.to_string();
    if id.is_empty() {
        return None;
    }
    let summary = item
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("(sem título)")
        .to_string();
    let (start_at, end_at) = parse_event_times(item);
    let description = item
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    let location = item
        .get("location")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    let form = parse_form_from_item(item);
    Some(CalendarEvent {
        id,
        calendar_id: PRIMARY.to_string(),
        summary,
        start_at,
        end_at,
        description,
        location,
        form,
    })
}

/// Corpo JSON para `events.insert` / `events.patch`.
fn build_event_value(
    summary: &str,
    all_day: bool,
    start_iso: &str,
    end_iso: &str,
    description: Option<&str>,
    location: Option<&str>,
    ext: &EventWriteExtensions,
    is_patch: bool,
) -> Value {
    let mut v = if all_day {
        json!({
            "summary": summary,
            "start": { "date": start_iso.trim() },
            "end": { "date": end_iso.trim() },
        })
    } else {
        json!({
            "summary": summary,
            "start": { "dateTime": start_iso.trim() },
            "end": { "dateTime": end_iso.trim() },
        })
    };
    let obj = v.as_object_mut().expect("object");
    if let Some(d) = description {
        let t = d.trim();
        if !t.is_empty() {
            obj.insert("description".to_string(), json!(t));
        } else if is_patch {
            obj.insert("description".to_string(), json!(null));
        }
    }
    if let Some(l) = location {
        let t = l.trim();
        if !t.is_empty() {
            obj.insert("location".to_string(), json!(t));
        } else if is_patch {
            obj.insert("location".to_string(), json!(null));
        }
    }

    if let Some(cid) = ext.color_id.as_ref() {
        let t = cid.trim();
        if !t.is_empty() {
            obj.insert("colorId".to_string(), json!(t));
        } else if is_patch {
            obj.insert("colorId".to_string(), json!(null));
        }
    }

    let tr = ext.transparency.as_str();
    if tr == "transparent" || tr == "opaque" {
        obj.insert("transparency".to_string(), json!(tr));
    }
    let vis = ext.visibility.as_str();
    if matches!(vis, "default" | "public" | "private" | "confidential") {
        obj.insert("visibility".to_string(), json!(vis));
    }

    obj.insert(
        "guestsCanModify".to_string(),
        json!(ext.guests_can_modify),
    );
    obj.insert(
        "guestsCanInviteOthers".to_string(),
        json!(ext.guests_can_invite_others),
    );
    obj.insert(
        "guestsCanSeeOtherGuests".to_string(),
        json!(ext.guests_can_see_other_guests),
    );

    let attendees = sanitize_attendees(&ext.attendees);
    if is_patch {
        obj.insert("attendees".to_string(), Value::Array(attendees));
    } else if !attendees.is_empty() {
        obj.insert("attendees".to_string(), Value::Array(attendees));
    }

    // Com `useDefault: true` é obrigatório não deixar `overrides` do evento anterior (PATCH faz merge).
    // Caso contrário a API devolve 400: cannotUseDefaultRemindersAndSpecifyOverride.
    if ext.use_default_reminders {
        obj.insert(
            "reminders".to_string(),
            json!({ "useDefault": true, "overrides": [] }),
        );
    } else {
        let mins = ext.reminder_minutes.unwrap_or(30).min(40320) as i64;
        obj.insert(
            "reminders".to_string(),
            json!({
                "useDefault": false,
                "overrides": [{ "method": "popup", "minutes": mins }]
            }),
        );
    }

    if let Some(rec) = recurrence_rules_from_ext(ext, start_iso, all_day, is_patch) {
        obj.insert("recurrence".to_string(), rec);
    }

    if ext.request_google_meet {
        obj.insert(
            "conferenceData".to_string(),
            json!({
                "createRequest": {
                    "requestId": meet_request_id(),
                    "conferenceSolutionKey": { "type": "hangoutsMeet" }
                }
            }),
        );
    }

    v
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn primary_events_url() -> Result<url::Url, String> {
    url::Url::parse(&format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events",
        PRIMARY
    ))
    .map_err(|e| e.to_string())
}

enum IncrementalOutcome {
    Ok(usize),
    /// HTTP 410 — token inválido; é preciso sync completa na janela de tempo.
    SyncTokenExpired,
}

/// Lista alterações desde o último `nextSyncToken` (sem apagar o resto da cache).
async fn sync_primary_incremental(
    app: &AppHandle,
    http: &reqwest::Client,
    access: &str,
    sync_token: &str,
) -> Result<IncrementalOutcome, String> {
    let mut page_token: Option<String> = None;
    let mut last_next_sync: Option<String> = None;
    let mut touched = 0usize;

    loop {
        let mut url = primary_events_url()?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("singleEvents", "true");
            q.append_pair("syncToken", sync_token);
            if let Some(ref pt) = page_token {
                q.append_pair("pageToken", pt);
            }
        }

        let res = http
            .get(url.as_str())
            .bearer_auth(access)
            .send()
            .await
            .map_err(|e| format!("Calendar API: {e}"))?;

        if res.status() == reqwest::StatusCode::GONE {
            return Ok(IncrementalOutcome::SyncTokenExpired);
        }

        if !res.status().is_success() {
            let t = res.text().await.unwrap_or_default();
            return Err(format!("Calendar API: {t}"));
        }

        let json: Value = res.json().await.map_err(|e| e.to_string())?;

        if let Some(items) = json.get("items").and_then(|x| x.as_array()) {
            for item in items {
                let id = match item.get("id").and_then(|v| v.as_str()) {
                    Some(s) if !s.is_empty() => s,
                    _ => continue,
                };
                let status = item.get("status").and_then(|v| v.as_str());
                if status == Some("cancelled") {
                    local_store::delete_cached_event(app, PRIMARY, id)?;
                    touched += 1;
                } else if let Some(ev) = event_from_api_item(item) {
                    local_store::upsert_cached_event(app, &ev)?;
                    touched += 1;
                }
            }
        }

        if let Some(nst) = json
            .get("nextSyncToken")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        {
            last_next_sync = Some(nst);
        }

        page_token = json
            .get("nextPageToken")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if page_token.is_none() {
            break;
        }
    }

    if let Some(ref tok) = last_next_sync {
        local_store::set_sync_state(app, PRIMARY, Some(tok.as_str()), now_ms())?;
    }

    Ok(IncrementalOutcome::Ok(touched))
}

/// Sync completa numa janela de tempo; grava `nextSyncToken` na última página.
async fn sync_primary_full_window(
    app: &AppHandle,
    http: &reqwest::Client,
    access: &str,
) -> Result<usize, String> {
    let now = Utc::now().date_naive();
    let start_d = now
        .checked_sub_months(Months::new(3))
        .unwrap_or(now);
    let end_d = now.checked_add_months(Months::new(6)).unwrap_or(now);
    let time_min = format!("{}T00:00:00Z", start_d);
    let time_max = format!("{}T23:59:59Z", end_d);

    let mut rows: Vec<CalendarEvent> = Vec::new();
    let mut page_token: Option<String> = None;
    let mut last_next_sync: Option<String> = None;

    loop {
        let mut url = primary_events_url()?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("singleEvents", "true");
            q.append_pair("orderBy", "startTime");
            q.append_pair("timeMin", &time_min);
            q.append_pair("timeMax", &time_max);
            if let Some(ref pt) = page_token {
                q.append_pair("pageToken", pt);
            }
        }

        let res = http
            .get(url.as_str())
            .bearer_auth(access)
            .send()
            .await
            .map_err(|e| format!("Calendar API: {e}"))?;

        if !res.status().is_success() {
            let t = res.text().await.unwrap_or_default();
            return Err(format!("Calendar API: {t}"));
        }

        let json: Value = res.json().await.map_err(|e| e.to_string())?;

        if let Some(items) = json.get("items").and_then(|x| x.as_array()) {
            for item in items {
                if let Some(ev) = event_from_api_item(item) {
                    rows.push(ev);
                }
            }
        }

        if let Some(nst) = json
            .get("nextSyncToken")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        {
            last_next_sync = Some(nst);
        }

        page_token = json
            .get("nextPageToken")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if page_token.is_none() {
            break;
        }
    }

    let n = rows.len();
    local_store::replace_calendar_events(app, PRIMARY, &rows)?;
    if let Some(ref tok) = last_next_sync {
        local_store::set_sync_state(app, PRIMARY, Some(tok.as_str()), now_ms())?;
    }
    Ok(n)
}

/// Sincroniza o calendário principal: incremental com `syncToken` quando existir; senão janela ~9 meses + token inicial.
pub async fn sync_primary_to_cache(app: &AppHandle) -> Result<usize, String> {
    let refresh = get_refresh_token(app)?;
    let access = refresh_access_token(app, &refresh).await?;

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    if let Some(sync_tok) = local_store::get_sync_token(app, PRIMARY)? {
        match sync_primary_incremental(app, &http, &access, &sync_tok).await? {
            IncrementalOutcome::Ok(n) => return Ok(n),
            IncrementalOutcome::SyncTokenExpired => {
                local_store::clear_sync_state_row(app, PRIMARY)?;
            }
        }
    }

    sync_primary_full_window(app, &http, &access).await
}

/// Cria um evento no calendário `primary`. `start_iso` / `end_iso`: RFC3339 com hora, ou só `YYYY-MM-DD` se `all_day` (fim **exclusivo** no último dia).
pub async fn create_primary_calendar_event(
    app: &AppHandle,
    summary: String,
    all_day: bool,
    start_iso: String,
    end_iso: String,
    description: Option<String>,
    location: Option<String>,
    extensions: EventWriteExtensions,
) -> Result<CalendarEvent, String> {
    let summary = summary.trim().to_string();
    if summary.is_empty() {
        return Err("Indica um título para o evento.".into());
    }
    let refresh = get_refresh_token(app)?;
    let access = refresh_access_token(app, &refresh).await?;

    let body = build_event_value(
        &summary,
        all_day,
        &start_iso,
        &end_iso,
        description.as_deref(),
        location.as_deref(),
        &extensions,
        false,
    );

    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events",
        PRIMARY
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client
        .post(&url)
        .query(&[("sendUpdates", "all")])
        .bearer_auth(access)
        .json(&body);
    if extensions.request_google_meet {
        req = req.query(&[("conferenceDataVersion", "1")]);
    }
    let res = req
        .send()
        .await
        .map_err(|e| format!("Calendar API: {e}"))?;

    if !res.status().is_success() {
        let t = res.text().await.unwrap_or_default();
        return Err(format_calendar_write_error("Não foi possível criar o evento", &t));
    }

    let created: Value = res.json().await.map_err(|e| e.to_string())?;
    let ev = event_from_api_item(&created)
        .ok_or_else(|| "Resposta inválida ao criar evento.".to_string())?;
    local_store::upsert_cached_event(app, &ev)?;
    Ok(ev)
}

fn calendar_event_resource_url(calendar_id: &str, event_id: &str) -> String {
    format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events/{}",
        urlenc(calendar_id),
        urlenc(event_id)
    )
}

pub async fn delete_calendar_event(
    app: &AppHandle,
    calendar_id: &str,
    event_id: &str,
) -> Result<(), String> {
    if event_id.trim().is_empty() {
        return Err("ID do evento em falta.".into());
    }
    let refresh = get_refresh_token(app)?;
    let access = refresh_access_token(app, &refresh).await?;
    let url = calendar_event_resource_url(calendar_id, event_id);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let res = client
        .delete(&url)
        .query(&[("sendUpdates", "all")])
        .bearer_auth(access)
        .send()
        .await
        .map_err(|e| format!("Calendar API: {e}"))?;
    if !res.status().is_success() {
        let t = res.text().await.unwrap_or_default();
        return Err(format_calendar_write_error("Não foi possível apagar o evento", &t));
    }
    // 204 No Content — sem JSON
    local_store::delete_cached_event(app, calendar_id, event_id)?;
    Ok(())
}

pub async fn update_calendar_event(
    app: &AppHandle,
    calendar_id: &str,
    event_id: &str,
    summary: String,
    all_day: bool,
    start_iso: String,
    end_iso: String,
    description: Option<String>,
    location: Option<String>,
    extensions: EventWriteExtensions,
) -> Result<CalendarEvent, String> {
    let summary = summary.trim().to_string();
    if summary.is_empty() {
        return Err("Indica um título para o evento.".into());
    }
    if event_id.trim().is_empty() {
        return Err("ID do evento em falta.".into());
    }
    let refresh = get_refresh_token(app)?;
    let access = refresh_access_token(app, &refresh).await?;

    let body = build_event_value(
        &summary,
        all_day,
        &start_iso,
        &end_iso,
        description.as_deref(),
        location.as_deref(),
        &extensions,
        true,
    );

    let url = calendar_event_resource_url(calendar_id, event_id);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client
        .patch(&url)
        .query(&[("sendUpdates", "all")])
        .bearer_auth(access)
        .json(&body);
    if extensions.request_google_meet {
        req = req.query(&[("conferenceDataVersion", "1")]);
    }
    let res = req
        .send()
        .await
        .map_err(|e| format!("Calendar API: {e}"))?;

    if !res.status().is_success() {
        let t = res.text().await.unwrap_or_default();
        return Err(format_calendar_write_error("Não foi possível atualizar o evento", &t));
    }

    let updated: Value = res.json().await.map_err(|e| e.to_string())?;
    let ev = event_from_api_item(&updated)
        .ok_or_else(|| "Resposta inválida ao atualizar evento.".to_string())?;
    local_store::upsert_cached_event(app, &ev)?;
    Ok(ev)
}

pub fn sign_out_and_clear_cache(app: &AppHandle) -> Result<(), String> {
    delete_refresh_token(app)?;
    local_store::clear_calendar_events(app, PRIMARY)?;
    Ok(())
}
