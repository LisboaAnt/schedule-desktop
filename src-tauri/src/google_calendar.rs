//! OAuth 2.0 Google (PKCE + callback loopback 127.0.0.1) e Calendar API v3 (cache SQLite).
//! Client ID (primeiro que existir): constante [`EMBEDDED_GOOGLE_OAUTH_CLIENT_ID`] (se preenchida),
//! depois **compile time** `GOOGLE_OAUTH_CLIENT_ID` no `cargo tauri build`, depois env em runtime,
//! depois ficheiro `google_oauth_client_id.txt` em `app_config_dir`. **Não é obrigatório usar `.env`.**
//! Fluxo recomendado para distribuição da app desktop: credencial OAuth **Desktop** (sem `client_secret`),
//! mantendo PKCE obrigatório. O callback é local (`http://127.0.0.1:<porta>/oauth2callback`).

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::{DateTime, Datelike, Months, NaiveDate, Utc, Weekday};
use keyring::Entry;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use urlencoding::encode as urlenc;
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use tauri::Manager;

use crate::calendar_model::{
    CalendarAttendee, CalendarEvent, CalendarEventForm, CreateGoogleEventPayload,
    DeleteGoogleEventPayload, EventWriteExtensions, QueuedCalendarMutation,
    UpdateGoogleEventPayload,
};
use crate::local_store;

const KEYRING_SERVICE: &str = "calendario-app";
const KEYRING_USER_REFRESH: &str = "google_oauth_refresh_token";
/// Ficheiro na pasta de dados local (ao lado do SQLite). O keyring no Windows em dev falha por vezes; o ficheiro garante persistência.
const REFRESH_TOKEN_FILENAME: &str = "google_oauth_refresh_token";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
/// Leitura + criação/edição de eventos + perfil (foto/e-mail). Após mudar o escopo, volta a **Ligar conta Google**.
/// Usa `openid profile email` (OIDC) em vez dos URLs `userinfo.*` — evita erros intermitentes na página de consentimento.
const SCOPE: &str = "openid email profile https://www.googleapis.com/auth/calendar.events";
const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
const PRIMARY: &str = "primary";
/// Prefixo do parâmetro `state` OAuth da **app Agenda**.
pub const OAUTH_STATE_PREFIX: &str = "agenda_";
/// Evita dois logins OAuth em simultâneo.
static OAUTH_FLOW_MUTEX: Mutex<()> = Mutex::new(());
/// Tempo máximo para concluir o OAuth no browser.
const OAUTH_CALLBACK_WAIT_SECS: u64 = 600;

/// ID de cliente OAuth Google (valor **público** na consola Google). Se colocares aqui o teu Client ID,
/// a app funciona sem `.env` e sem `google_oauth_client_id.txt` — útil quando o `.env` não é carregado no Tauri.
/// Deixa vazio `""` se usares só variável na build ou ficheiro na pasta de configuração.
const EMBEDDED_GOOGLE_OAUTH_CLIENT_ID: &str =
    "996263499952-5tdh2d08f0hril3o78bqt5dvg06pop7o.apps.googleusercontent.com";
/// Endpoint servidor que faz token exchange/refresh com client_secret protegido.
const OAUTH_TOKEN_BRIDGE_DEFAULT: &str = "https://www.alemsys.digital/api/auth/google/token";

#[derive(Debug, Deserialize)]
struct TokenJson {
    #[serde(alias = "accessToken")]
    access_token: String,
    #[serde(alias = "refreshToken")]
    refresh_token: Option<String>,
    #[allow(dead_code)]
    #[serde(alias = "expiresIn")]
    expires_in: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GoogleUserProfile {
    pub email: Option<String>,
    pub name: Option<String>,
    pub picture: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeAuthCodeRequest<'a> {
    grant_type: &'a str,
    code: &'a str,
    code_verifier: &'a str,
    redirect_uri: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeRefreshRequest<'a> {
    grant_type: &'a str,
    refresh_token: &'a str,
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
/// Ordem: [`EMBEDDED_GOOGLE_OAUTH_CLIENT_ID`] → `option_env!("GOOGLE_OAUTH_CLIENT_ID")` na build →
/// `GOOGLE_OAUTH_CLIENT_ID` em runtime → `google_oauth_client_id.txt` na pasta de config.
pub fn resolve_client_id(app: &AppHandle) -> Result<String, String> {
    let embedded = EMBEDDED_GOOGLE_OAUTH_CLIENT_ID.trim();
    if !embedded.is_empty() {
        return Ok(embedded.to_string());
    }
    if let Some(v) = option_env!("GOOGLE_OAUTH_CLIENT_ID") {
        let t = v.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    if let Ok(v) = std::env::var("GOOGLE_OAUTH_CLIENT_ID") {
        let t = v.trim().to_string();
        if !t.is_empty() {
            return Ok(t);
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
        "OAuth: preenche EMBEDDED_GOOGLE_OAUTH_CLIENT_ID em google_calendar.rs, ou define GOOGLE_OAUTH_CLIENT_ID na build, ou cria google_oauth_client_id.txt na pasta de configuração da app."
            .into(),
    )
}

pub fn client_id_configured(app: &AppHandle) -> bool {
    resolve_client_id(app).is_ok()
}

fn resolve_oauth_token_bridge_url() -> String {
    if let Ok(v) = std::env::var("GOOGLE_OAUTH_TOKEN_BRIDGE_URL") {
        let t = v.trim().to_string();
        if !t.is_empty() {
            return t;
        }
    }
    if let Some(v) = option_env!("GOOGLE_OAUTH_TOKEN_BRIDGE_URL") {
        let t = v.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    OAUTH_TOKEN_BRIDGE_DEFAULT.to_string()
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

/// Compatibilidade com chamadas antigas de deep link; o fluxo atual usa callback loopback local.
pub fn try_complete_oauth_from_deep_link(_url_str: &str) {
}

fn wait_for_oauth_code_loopback(listener: &TcpListener, expected_state: &str) -> Result<String, String> {
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("OAuth: configurar callback local: {e}"))?;
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > Duration::from_secs(OAUTH_CALLBACK_WAIT_SECS) {
            return Err(
                "OAuth: tempo esgotado à espera do callback local. Mantém a Agenda aberta e termina o login no browser."
                    .into(),
            );
        }
        match listener.accept() {
            Ok((mut stream, _addr)) => {
                let mut first_line = String::new();
                {
                    let mut reader = BufReader::new(&mut stream);
                    let _ = reader.read_line(&mut first_line);
                }
                let path = first_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/");
                let callback_url = format!("http://127.0.0.1{path}");
                let parsed = url::Url::parse(&callback_url)
                    .map_err(|e| format!("OAuth: callback inválido: {e}"))?;
                let mut oauth_error: Option<String> = None;
                let mut err_desc: Option<String> = None;
                let mut code: Option<String> = None;
                let mut state: Option<String> = None;
                for (k, v) in parsed.query_pairs() {
                    match k.as_ref() {
                        "error" => oauth_error = Some(v.into_owned()),
                        "error_description" => err_desc = Some(v.into_owned()),
                        "code" => code = Some(v.into_owned()),
                        "state" => state = Some(v.into_owned()),
                        _ => {}
                    }
                }
                let ok_html = r#"<!doctype html>
<html lang="pt">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Agenda — Login concluído</title>
  <style>
    :root { color-scheme: dark light; }
    body {
      margin: 0;
      min-height: 100dvh;
      display: grid;
      place-items: center;
      font-family: Inter, Segoe UI, Roboto, Arial, sans-serif;
      background: radial-gradient(1200px 600px at 20% -10%, #6b8afd22, transparent 60%),
                  radial-gradient(900px 500px at 120% 110%, #41d1ff22, transparent 55%),
                  #0b1020;
      color: #e8ecff;
    }
    .card {
      width: min(560px, 92vw);
      background: #121936;
      border: 1px solid #2a356a;
      border-radius: 16px;
      box-shadow: 0 10px 40px #00000055;
      padding: 28px 24px;
      text-align: center;
    }
    .ok {
      width: 52px; height: 52px; margin: 0 auto 14px;
      border-radius: 50%;
      display: grid; place-items: center;
      background: #1f8f4e33; border: 1px solid #41d17d66;
      color: #7dffb2; font-size: 28px; font-weight: 700;
    }
    h1 { margin: 0 0 10px; font-size: 22px; line-height: 1.3; }
    p { margin: 0; color: #b7c0ea; line-height: 1.5; }
    .muted { margin-top: 10px; font-size: 13px; opacity: 0.9; }
    button {
      margin-top: 18px;
      border: 1px solid #3a4c95;
      background: #1a2550;
      color: #e8ecff;
      border-radius: 10px;
      padding: 10px 14px;
      cursor: pointer;
      font-weight: 600;
    }
    button:hover { background: #22306a; }
  </style>
</head>
<body>
  <main class="card" role="main" aria-live="polite">
    <div class="ok">✓</div>
    <h1>Login com Google concluído</h1>
    <p>Podes voltar para a app Agenda. Esta aba pode ser fechada.</p>
    <p class="muted">Se a app não atualizar automaticamente, clica em “Sincronizar”.</p>
    <button type="button" onclick="window.close()">Fechar aba</button>
  </main>
  <script>setTimeout(function(){ try{ window.close(); }catch(_){} }, 1200);</script>
</body>
</html>"#;
                let fail_html = r#"<!doctype html>
<html lang="pt">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Agenda — Falha no login</title>
  <style>
    body {
      margin: 0; min-height: 100dvh; display: grid; place-items: center;
      font-family: Inter, Segoe UI, Roboto, Arial, sans-serif;
      background: #120d1a; color: #f8e9ee;
    }
    .card {
      width: min(560px, 92vw);
      background: #231126;
      border: 1px solid #5a2b56;
      border-radius: 16px;
      padding: 24px;
      text-align: center;
    }
    h1 { margin: 0 0 10px; font-size: 22px; }
    p { margin: 0; color: #f3cdd8; line-height: 1.5; }
  </style>
</head>
<body>
  <main class="card" role="main" aria-live="polite">
    <h1>Falha no login Google</h1>
    <p>Volta para a app Agenda e tenta novamente.</p>
  </main>
</body>
</html>"#;
                let (status_line, body, result) = if let Some(err) = oauth_error {
                    let detail = err_desc.unwrap_or_default();
                    let msg = if detail.is_empty() { err } else { format!("{err}: {detail}") };
                    ("HTTP/1.1 400 Bad Request\r\n", fail_html, Err(format!("OAuth recusado: {msg}")))
                } else if state.as_deref() != Some(expected_state) {
                    (
                        "HTTP/1.1 400 Bad Request\r\n",
                        fail_html,
                        Err("OAuth: estado inválido no callback local.".into()),
                    )
                } else if let Some(c) = code {
                    ("HTTP/1.1 200 OK\r\n", ok_html, Ok(c))
                } else {
                    (
                        "HTTP/1.1 400 Bad Request\r\n",
                        fail_html,
                        Err("OAuth: código em falta no callback local.".into()),
                    )
                };
                let response = format!(
                    "{status_line}Content-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
                return result;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(120));
            }
            Err(e) => return Err(format!("OAuth: callback local falhou: {e}")),
        }
    }
}

/// Abre o browser, recebe o código por callback local (loopback) e troca por tokens.
pub fn run_desktop_oauth_flow(app: &AppHandle, client_id: &str) -> Result<(), String> {
    let _flow_guard = OAUTH_FLOW_MUTEX.try_lock().map_err(|e| match e {
        std::sync::TryLockError::WouldBlock => {
            "OAuth: já há um login em curso. Completa no browser ou aguarda; não abras dois logins ao mesmo tempo.".to_string()
        }
        std::sync::TryLockError::Poisoned(_) => "OAuth: reinicia a app e tenta de novo.".to_string(),
    })?;

    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("OAuth: não foi possível abrir callback local: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("OAuth: local_addr callback: {e}"))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/oauth2callback");
    let verifier = pkce_verifier();
    let challenge = pkce_challenge_s256(&verifier);
    let state = format!("{OAUTH_STATE_PREFIX}{}", random_state());
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
    if let Err(e) = open::that(url_str) {
        return Err(format!("OAuth: abrir browser: {e}"));
    }
    let code = wait_for_oauth_code_loopback(&listener, &state)?;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let bridge_url = resolve_oauth_token_bridge_url();
    let token_res = client
        .post(bridge_url)
        .json(&BridgeAuthCodeRequest {
            grant_type: "authorization_code",
            code: code.as_str(),
            code_verifier: verifier.as_str(),
            redirect_uri: redirect_uri.as_str(),
        })
        .send()
        .map_err(|e| format!("OAuth bridge: token HTTP: {e}"))?;

    if !token_res.status().is_success() {
        let t = token_res.text().unwrap_or_default();
        let hint = if t.contains("redirect_uri_mismatch") {
            " Confirma no Google Cloud que callbacks loopback locais estão permitidos."
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

pub async fn refresh_access_token(_app: &AppHandle, refresh: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let bridge_url = resolve_oauth_token_bridge_url();
    let token_res = client
        .post(bridge_url)
        .json(&BridgeRefreshRequest {
            grant_type: "refresh_token",
            refresh_token: refresh,
        })
        .send()
        .await
        .map_err(|e| format!("token bridge: {e}"))?;

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

pub async fn get_google_user_profile(app: &AppHandle) -> Result<GoogleUserProfile, String> {
    let refresh = get_refresh_token(app)?;
    let access = refresh_access_token(app, &refresh).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let res = client
        .get(USERINFO_URL)
        .bearer_auth(access)
        .send()
        .await
        .map_err(|e| format!("Google userinfo: {e}"))?;
    if !res.status().is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Google userinfo falhou: {body}"));
    }
    let profile: GoogleUserProfile = res
        .json()
        .await
        .map_err(|e| format!("Google userinfo JSON: {e}"))?;
    Ok(profile)
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

/// Erro ao escrever na API: transitório (rede, 5xx, 429) vs permanente (4xx de validação, auth na API).
#[derive(Debug)]
pub enum CalendarMutationError {
    Transient(String),
    Permanent(String),
}

fn http_status_is_transient(status: reqwest::StatusCode) -> bool {
    status.is_server_error()
        || status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
}

fn transient_queue_suffix() -> &'static str {
    " A alteração ficou na fila offline; sincroniza a agenda para tentar enviar."
}

fn map_send_error(e: reqwest::Error) -> CalendarMutationError {
    CalendarMutationError::Transient(format!("Calendar API: {e}"))
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
#[allow(clippy::too_many_arguments)]
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
    if is_patch || !attendees.is_empty() {
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

/// Cria um evento no calendário `primary`. Em falha de rede / 5xx / 429, enfileira e devolve mensagem com aviso da fila.
pub async fn create_primary_calendar_event(
    app: &AppHandle,
    payload: CreateGoogleEventPayload,
) -> Result<CalendarEvent, String> {
    match create_primary_calendar_event_impl(app, &payload).await {
        Ok(ev) => Ok(ev),
        Err(CalendarMutationError::Transient(e)) => {
            let q = QueuedCalendarMutation::Create(payload.clone());
            local_store::pending_mutations_enqueue(app, &q)?;
            Err(format!("{e}{}", transient_queue_suffix()))
        }
        Err(CalendarMutationError::Permanent(e)) => Err(e),
    }
}

async fn create_primary_calendar_event_impl(
    app: &AppHandle,
    payload: &CreateGoogleEventPayload,
) -> Result<CalendarEvent, CalendarMutationError> {
    let summary = payload.summary.trim().to_string();
    if summary.is_empty() {
        return Err(CalendarMutationError::Permanent(
            "Indica um título para o evento.".into(),
        ));
    }
    let refresh = get_refresh_token(app).map_err(CalendarMutationError::Permanent)?;
    let access = refresh_access_token(app, &refresh)
        .await
        .map_err(CalendarMutationError::Permanent)?;

    let body = build_event_value(
        &summary,
        payload.all_day,
        &payload.start_iso,
        &payload.end_iso,
        payload.description.as_deref(),
        payload.location.as_deref(),
        &payload.extensions,
        false,
    );

    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events",
        PRIMARY
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| CalendarMutationError::Permanent(e.to_string()))?;

    let mut req = client
        .post(&url)
        .query(&[("sendUpdates", "all")])
        .bearer_auth(access)
        .json(&body);
    if payload.extensions.request_google_meet {
        req = req.query(&[("conferenceDataVersion", "1")]);
    }
    let res = req.send().await.map_err(map_send_error)?;

    let status = res.status();
    if !status.is_success() {
        let t = res.text().await.unwrap_or_default();
        if http_status_is_transient(status) {
            return Err(CalendarMutationError::Transient(format!(
                "Não foi possível criar o evento (HTTP {}): {}",
                status.as_u16(),
                t.trim()
            )));
        }
        return Err(CalendarMutationError::Permanent(format_calendar_write_error(
            "Não foi possível criar o evento",
            &t,
        )));
    }

    let created: Value = res
        .json()
        .await
        .map_err(|e| CalendarMutationError::Permanent(e.to_string()))?;
    let ev = event_from_api_item(&created).ok_or_else(|| {
        CalendarMutationError::Permanent("Resposta inválida ao criar evento.".into())
    })?;
    local_store::upsert_cached_event(app, &ev).map_err(CalendarMutationError::Permanent)?;
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
    payload: DeleteGoogleEventPayload,
) -> Result<(), String> {
    match delete_calendar_event_impl(app, &payload).await {
        Ok(()) => Ok(()),
        Err(CalendarMutationError::Transient(e)) => {
            let q = QueuedCalendarMutation::Delete(payload.clone());
            local_store::pending_mutations_enqueue(app, &q)?;
            Err(format!("{e}{}", transient_queue_suffix()))
        }
        Err(CalendarMutationError::Permanent(e)) => Err(e),
    }
}

async fn delete_calendar_event_impl(
    app: &AppHandle,
    payload: &DeleteGoogleEventPayload,
) -> Result<(), CalendarMutationError> {
    let calendar_id = payload.calendar_id.as_str();
    let event_id = payload.event_id.as_str();
    if event_id.trim().is_empty() {
        return Err(CalendarMutationError::Permanent(
            "ID do evento em falta.".into(),
        ));
    }
    let refresh = get_refresh_token(app).map_err(CalendarMutationError::Permanent)?;
    let access = refresh_access_token(app, &refresh)
        .await
        .map_err(CalendarMutationError::Permanent)?;
    let url = calendar_event_resource_url(calendar_id, event_id);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| CalendarMutationError::Permanent(e.to_string()))?;
    let res = client
        .delete(&url)
        .query(&[("sendUpdates", "all")])
        .bearer_auth(access)
        .send()
        .await
        .map_err(map_send_error)?;
    let status = res.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        local_store::delete_cached_event(app, calendar_id, event_id)
            .map_err(CalendarMutationError::Permanent)?;
        return Ok(());
    }
    if !status.is_success() {
        let t = res.text().await.unwrap_or_default();
        if http_status_is_transient(status) {
            return Err(CalendarMutationError::Transient(format!(
                "Não foi possível apagar o evento (HTTP {}): {}",
                status.as_u16(),
                t.trim()
            )));
        }
        return Err(CalendarMutationError::Permanent(format_calendar_write_error(
            "Não foi possível apagar o evento",
            &t,
        )));
    }
    local_store::delete_cached_event(app, calendar_id, event_id).map_err(CalendarMutationError::Permanent)?;
    Ok(())
}

pub async fn update_calendar_event(
    app: &AppHandle,
    payload: UpdateGoogleEventPayload,
) -> Result<CalendarEvent, String> {
    match update_calendar_event_impl(app, &payload).await {
        Ok(ev) => Ok(ev),
        Err(CalendarMutationError::Transient(e)) => {
            let q = QueuedCalendarMutation::Update(payload.clone());
            local_store::pending_mutations_enqueue(app, &q)?;
            Err(format!("{e}{}", transient_queue_suffix()))
        }
        Err(CalendarMutationError::Permanent(e)) => Err(e),
    }
}

async fn update_calendar_event_impl(
    app: &AppHandle,
    payload: &UpdateGoogleEventPayload,
) -> Result<CalendarEvent, CalendarMutationError> {
    let summary = payload.summary.trim().to_string();
    if summary.is_empty() {
        return Err(CalendarMutationError::Permanent(
            "Indica um título para o evento.".into(),
        ));
    }
    if payload.event_id.trim().is_empty() {
        return Err(CalendarMutationError::Permanent(
            "ID do evento em falta.".into(),
        ));
    }
    let refresh = get_refresh_token(app).map_err(CalendarMutationError::Permanent)?;
    let access = refresh_access_token(app, &refresh)
        .await
        .map_err(CalendarMutationError::Permanent)?;

    let body = build_event_value(
        &summary,
        payload.all_day,
        &payload.start_iso,
        &payload.end_iso,
        payload.description.as_deref(),
        payload.location.as_deref(),
        &payload.extensions,
        true,
    );

    let url = calendar_event_resource_url(&payload.calendar_id, &payload.event_id);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| CalendarMutationError::Permanent(e.to_string()))?;

    let mut req = client
        .patch(&url)
        .query(&[("sendUpdates", "all")])
        .bearer_auth(access)
        .json(&body);
    if payload.extensions.request_google_meet {
        req = req.query(&[("conferenceDataVersion", "1")]);
    }
    let res = req.send().await.map_err(map_send_error)?;

    let status = res.status();
    if !status.is_success() {
        let t = res.text().await.unwrap_or_default();
        if http_status_is_transient(status) {
            return Err(CalendarMutationError::Transient(format!(
                "Não foi possível atualizar o evento (HTTP {}): {}",
                status.as_u16(),
                t.trim()
            )));
        }
        return Err(CalendarMutationError::Permanent(format_calendar_write_error(
            "Não foi possível atualizar o evento",
            &t,
        )));
    }

    let updated: Value = res
        .json()
        .await
        .map_err(|e| CalendarMutationError::Permanent(e.to_string()))?;
    let ev = event_from_api_item(&updated).ok_or_else(|| {
        CalendarMutationError::Permanent("Resposta inválida ao atualizar evento.".into())
    })?;
    local_store::upsert_cached_event(app, &ev).map_err(CalendarMutationError::Permanent)?;
    Ok(ev)
}

/// Processa mutações pendentes (FIFO). Para em erro transitório; remove entradas com erro permanente.
pub async fn flush_pending_mutations(app: &AppHandle) -> Result<u32, String> {
    let mut done = 0u32;
    loop {
        let Some((row_id, json)) = local_store::pending_mutations_peek_first(app)? else {
            break;
        };
        let op: QueuedCalendarMutation = match serde_json::from_str(&json) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("[agenda] fila offline: payload inválido, a remover: {e}");
                local_store::pending_mutations_remove(app, row_id)?;
                continue;
            }
        };

        let result = match op {
            QueuedCalendarMutation::Create(ref p) => {
                create_primary_calendar_event_impl(app, p).await.map(|_| ())
            }
            QueuedCalendarMutation::Update(ref p) => {
                update_calendar_event_impl(app, p).await.map(|_| ())
            }
            QueuedCalendarMutation::Delete(ref p) => delete_calendar_event_impl(app, p).await,
        };

        match result {
            Ok(()) => {
                local_store::pending_mutations_remove(app, row_id)?;
                done += 1;
            }
            Err(CalendarMutationError::Transient(e)) => {
                let _ = local_store::pending_mutations_set_last_error(app, row_id, Some(&e));
                break;
            }
            Err(CalendarMutationError::Permanent(e)) => {
                eprintln!("[agenda] fila offline: a descartar entrada (erro permanente): {e}");
                local_store::pending_mutations_remove(app, row_id)?;
            }
        }
    }
    Ok(done)
}

pub fn sign_out_and_clear_cache(app: &AppHandle) -> Result<(), String> {
    delete_refresh_token(app)?;
    local_store::clear_calendar_events(app, PRIMARY)?;
    local_store::pending_mutations_clear(app)?;
    Ok(())
}
