//! OAuth 2.0 Google (PKCE + localhost) e Calendar API v3 (cache SQLite).
//! Client ID (primeiro que existir): variável de ambiente em runtime `GOOGLE_OAUTH_CLIENT_ID`,
//! ou em **compile time** `GOOGLE_OAUTH_CLIENT_ID` ao correr `cargo`/`tauri build`, ou ficheiro
//! `google_oauth_client_id.txt` em `app_config_dir`.
//! **Client secret** (`GOOGLE_OAUTH_CLIENT_SECRET` ou `google_oauth_client_secret.txt`): só se o cliente
//! OAuth na Google estiver como **aplicação Web**; o tipo **Desktop** funciona só com PKCE (sem secret).
//! Redirect fixo: `http://127.0.0.1:17892/callback` — adiciona este URI nas credenciais OAuth (app desktop).

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::{Months, Utc};
use keyring::Entry;
use rand::Rng;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use tauri::Manager;

use crate::calendar_model::CalendarEvent;
use crate::local_store;

const KEYRING_SERVICE: &str = "calendario-app";
const KEYRING_USER_REFRESH: &str = "google_oauth_refresh_token";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "https://www.googleapis.com/auth/calendar.readonly";
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

pub fn has_refresh_token() -> bool {
    keyring_entry()
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|s| !s.is_empty())
        .is_some()
}

fn store_refresh_token(token: &str) -> Result<(), String> {
    let e = keyring_entry()?;
    e.set_password(token).map_err(|e| e.to_string())
}

pub fn delete_refresh_token() -> Result<(), String> {
    let e = keyring_entry()?;
    let _ = e.delete_credential();
    Ok(())
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

    store_refresh_token(&refresh)?;
    let _ = app;
    Ok(())
}

fn get_refresh_token() -> Result<String, String> {
    let e = keyring_entry()?;
    e.get_password().map_err(|e| e.to_string())
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
    Some(CalendarEvent {
        id,
        calendar_id: PRIMARY.to_string(),
        summary,
        start_at,
        end_at,
    })
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
    let refresh = get_refresh_token()?;
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

pub fn sign_out_and_clear_cache(app: &AppHandle) -> Result<(), String> {
    delete_refresh_token()?;
    local_store::clear_calendar_events(app, PRIMARY)?;
    Ok(())
}
