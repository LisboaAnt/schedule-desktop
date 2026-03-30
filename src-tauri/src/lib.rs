use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

mod calendar_model;
mod google_calendar;
mod local_store;

#[cfg(windows)]
mod windows_desktop;

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
use tauri::PhysicalPosition;

#[cfg(windows)]
use tauri::{WebviewUrl, WebviewWindowBuilder};

/// Janela principal está ancorada atrás dos ícones do ambiente de trabalho (WorkerW).
#[cfg(windows)]
static DESKTOP_WALLPAPER_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Janela-pílula: quadrado ~ícone do ambiente de trabalho (lógico).
#[cfg(windows)]
const RESTORE_PILL_INNER_LOGICAL: f64 = 14.0;

#[cfg(windows)]
const JS_WALLPAPER_ENTER: &str = "document.documentElement.classList.add('wallpaper-calendar-only');document.body.classList.add('desktop-wallpaper-mode');";

#[cfg(windows)]
const JS_WALLPAPER_LEAVE: &str = "document.documentElement.classList.remove('wallpaper-calendar-only');document.body.classList.remove('desktop-wallpaper-mode');";

#[cfg(windows)]
const JS_WALLPAPER_REPOSITION_PILL: &str = "requestAnimationFrame(function(){requestAnimationFrame(function(){if(window.__agendaRepositionRestorePill)window.__agendaRepositionRestorePill();});});";

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub view_mode: String,
    pub theme: String,
    #[serde(default = "default_opacity")]
    pub widget_opacity: f64,
    #[serde(default = "default_agenda_view")]
    pub agenda_view: String,
    #[serde(default)]
    pub desktop_behind_icons: bool,
    /// 0 = desligado; ex.: 5, 15, 30, 60 (minutos) para sincronizar com a API em segundo plano.
    #[serde(default = "default_auto_sync_minutes")]
    pub auto_sync_minutes: u32,
    /// Fechar a janela principal apenas oculta; sair pela bandeja.
    #[serde(default)]
    pub close_to_tray: bool,
}

fn default_auto_sync_minutes() -> u32 {
    0
}

fn default_opacity() -> f64 {
    1.0
}

fn default_agenda_view() -> String {
    "month".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            view_mode: "widget".to_string(),
            theme: "dark".to_string(),
            widget_opacity: 1.0,
            agenda_view: default_agenda_view(),
            desktop_behind_icons: false,
            auto_sync_minutes: 0,
            close_to_tray: false,
        }
    }
}

fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("path app_config_dir: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("create_dir_all: {e}"))?;
    Ok(dir.join("config.json"))
}

fn read_config_file(app: &tauri::AppHandle) -> Result<AppConfig, String> {
    let path = config_path(app)?;
    if path.exists() {
        let s = fs::read_to_string(&path).map_err(|e| format!("read config: {e}"))?;
        serde_json::from_str(&s).map_err(|e| format!("parse config: {e}"))
    } else {
        Ok(AppConfig::default())
    }
}

fn write_config_file(app: &tauri::AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = config_path(app)?;
    let s =
        serde_json::to_string_pretty(config).map_err(|e| format!("serialize config: {e}"))?;
    fs::write(&path, s).map_err(|e| format!("write config: {e}"))?;
    Ok(())
}

fn undo_desktop_behind_if_needed(app: &tauri::AppHandle) {
    let Ok(mut cfg) = read_config_file(app) else {
        return;
    };
    if !cfg.desktop_behind_icons {
        return;
    }
    cfg.desktop_behind_icons = false;
    #[cfg(windows)]
    if let Some(win) = app.get_webview_window("main") {
        let _ = windows_desktop::set_behind_icons(&win, false);
    }
    let _ = write_config_file(app, &cfg);
}

/// Ao arrancar: desfazer modo “atrás dos ícones”, fechar pílula de restauro e limpar CSS.
fn undo_desktop_wallpaper_on_launch(app: &tauri::AppHandle) {
    #[cfg(windows)]
    {
        DESKTOP_WALLPAPER_ACTIVE.store(false, Ordering::SeqCst);
        if let Some(pill) = app.get_webview_window("restore-pill") {
            let _ = pill.close();
        }
        if let Some(main) = app.get_webview_window("main") {
            let _ = windows_desktop::set_behind_icons(&main, false);
            let _ = main.eval(JS_WALLPAPER_LEAVE);
        }
    }
    undo_desktop_behind_if_needed(app);
}

#[cfg(windows)]
fn position_restore_pill<R: Runtime>(
    main: &tauri::WebviewWindow<R>,
    pill: &tauri::WebviewWindow<R>,
) -> Result<(), String> {
    let sz = pill.outer_size().map_err(|e| e.to_string())?;
    let w = sz.width as i32;
    let h = sz.height as i32;
    let hwnd = main.hwnd().map_err(|e| e.to_string())?;
    let (x, y) = windows_desktop::physical_position_for_pill_beside_main(hwnd, w, h)?;
    pill
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(windows)]
fn restore_desktop_wallpaper_mode_internal<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    DESKTOP_WALLPAPER_ACTIVE.store(false, Ordering::SeqCst);
    if let Some(pill) = app.get_webview_window("restore-pill") {
        let _ = pill.close();
    }
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "Janela principal em falta.".to_string())?;
    windows_desktop::set_behind_icons(&main, false)?;
    let _ = main.eval(JS_WALLPAPER_LEAVE);
    main.set_always_on_bottom(false).map_err(|e| e.to_string())?;
    let _ = main.unminimize();
    let _ = main.show();
    main.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

fn bring_main_window_forward<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    #[cfg(windows)]
    if DESKTOP_WALLPAPER_ACTIVE.load(Ordering::SeqCst) {
        return restore_desktop_wallpaper_mode_internal(app);
    }
    let w = app
        .get_webview_window("main")
        .ok_or_else(|| "Janela principal em falta.".to_string())?;
    w.set_always_on_bottom(false)
        .map_err(|e| e.to_string())?;
    let _ = w.unminimize();
    let _ = w.show();
    w.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(windows, allow(dead_code))]
fn send_main_window_back<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let w = app
        .get_webview_window("main")
        .ok_or_else(|| "Janela principal em falta.".to_string())?;
    w.set_always_on_bottom(true).map_err(|e| e.to_string())?;
    Ok(())
}

fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(
        app,
        "tray_bring_front",
        "Mostrar agenda",
        true,
        Option::<&str>::None,
    )?;
    let hide = MenuItem::with_id(
        app,
        "tray_hide",
        "Ocultar janela",
        true,
        Option::<&str>::None,
    )?;
    let quit = MenuItem::with_id(
        app,
        "tray_quit",
        "Sair",
        true,
        Option::<&str>::None,
    )?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .item(&hide)
        .separator()
        .item(&quit)
        .build()?;

    let show_id = show.id().clone();
    let hide_id = hide.id().clone();
    let quit_id = quit.id().clone();

    let icon = match app.default_window_icon() {
        Some(i) => i.clone(),
        None => Image::from_bytes(include_bytes!("../icons/32x32.png"))?,
    };

    TrayIconBuilder::new()
        .menu(&menu)
        .icon(icon)
        .tooltip("Agenda — clique para mostrar")
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| {
            if event.id == show_id {
                let _ = bring_main_window_forward(app);
            } else if event.id == hide_id {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            } else if event.id == quit_id {
                app.exit(0);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = bring_main_window_forward(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

/// Liga ou desliga o arranque com o sistema (Windows: registo de arranque; macOS/Linux: plugin autostart).
#[tauri::command]
async fn autostart_set(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let m = app.autolaunch();
    if enabled {
        m.enable().map_err(|e| e.to_string())
    } else {
        m.disable().map_err(|e| e.to_string())
    }
}

#[tauri::command]
async fn autostart_is_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_app_config(app: tauri::AppHandle) -> Result<AppConfig, String> {
    read_config_file(&app)
}

#[tauri::command]
fn save_app_config(app: tauri::AppHandle, config: AppConfig) -> Result<(), String> {
    write_config_file(&app, &config)
}

/// Estado da ligação à agenda (Fase 2: OAuth + Google).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarState {
    /// `demo` ou `google` conforme exista refresh token guardado.
    pub source: String,
    pub connected: bool,
    pub db_ready: bool,
    /// `true` se `GOOGLE_OAUTH_CLIENT_ID` ou `google_oauth_client_id.txt` estiver definido.
    #[serde(default)]
    pub client_id_configured: bool,
}

#[tauri::command]
fn get_calendar_state(app: tauri::AppHandle) -> CalendarState {
    let connected = google_calendar::has_refresh_token(&app);
    CalendarState {
        source: if connected {
            "google".to_string()
        } else {
            "demo".to_string()
        },
        connected,
        db_ready: local_store::is_ready(),
        client_id_configured: google_calendar::client_id_configured(&app),
    }
}

#[tauri::command]
async fn google_calendar_sign_in(app: tauri::AppHandle) -> Result<(), String> {
    let client_id = google_calendar::resolve_client_id(&app)?;
    let app_clone = app.clone();
    tokio::task::spawn_blocking(move || {
        google_calendar::run_desktop_oauth_flow(&app_clone, &client_id)
    })
    .await
    .map_err(|e| format!("OAuth: {e}"))?
}

#[tauri::command]
async fn google_calendar_sync(app: tauri::AppHandle) -> Result<u32, String> {
    let n = google_calendar::sync_primary_to_cache(&app).await?;
    Ok(n as u32)
}

#[tauri::command]
fn google_calendar_disconnect(app: tauri::AppHandle) -> Result<(), String> {
    google_calendar::sign_out_and_clear_cache(&app)
}

#[tauri::command]
fn get_cached_calendar_events(
    app: tauri::AppHandle,
) -> Result<Vec<calendar_model::CalendarEvent>, String> {
    local_store::list_cached_events(&app)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateGoogleEventPayload {
    pub summary: String,
    pub all_day: bool,
    pub start_iso: String,
    pub end_iso: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub extensions: calendar_model::EventWriteExtensions,
}

#[tauri::command]
async fn google_calendar_create_event(
    app: tauri::AppHandle,
    payload: CreateGoogleEventPayload,
) -> Result<calendar_model::CalendarEvent, String> {
    google_calendar::create_primary_calendar_event(
        &app,
        payload.summary,
        payload.all_day,
        payload.start_iso,
        payload.end_iso,
        payload.description,
        payload.location,
        payload.extensions,
    )
    .await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGoogleEventPayload {
    pub calendar_id: String,
    pub event_id: String,
    pub summary: String,
    pub all_day: bool,
    pub start_iso: String,
    pub end_iso: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub extensions: calendar_model::EventWriteExtensions,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteGoogleEventPayload {
    pub calendar_id: String,
    pub event_id: String,
}

#[tauri::command]
async fn google_calendar_update_event(
    app: tauri::AppHandle,
    payload: UpdateGoogleEventPayload,
) -> Result<calendar_model::CalendarEvent, String> {
    google_calendar::update_calendar_event(
        &app,
        &payload.calendar_id,
        &payload.event_id,
        payload.summary,
        payload.all_day,
        payload.start_iso,
        payload.end_iso,
        payload.description,
        payload.location,
        payload.extensions,
    )
    .await
}

#[tauri::command]
async fn google_calendar_delete_event(
    app: tauri::AppHandle,
    payload: DeleteGoogleEventPayload,
) -> Result<(), String> {
    google_calendar::delete_calendar_event(&app, &payload.calendar_id, &payload.event_id).await
}

/// No Windows: ancora atrás dos ícones do ambiente de trabalho, esconde a barra (CSS) e abre a pílula “voltar”.
/// Comando `async` para evitar deadlock do Webview2 ao criar a segunda janela.
#[tauri::command]
async fn send_window_to_back(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        let main = app
            .get_webview_window("main")
            .ok_or_else(|| "Janela principal em falta.".to_string())?;
        main
            .eval(JS_WALLPAPER_ENTER)
            .map_err(|e| e.to_string())?;
        windows_desktop::set_behind_icons(&main, true)?;
        DESKTOP_WALLPAPER_ACTIVE.store(true, Ordering::SeqCst);

        if let Some(p) = app.get_webview_window("restore-pill") {
            p.show().map_err(|e| e.to_string())?;
        } else {
            WebviewWindowBuilder::new(&app, "restore-pill", WebviewUrl::App("restore-pill.html".into()))
                .title("Agenda")
                .inner_size(RESTORE_PILL_INNER_LOGICAL, RESTORE_PILL_INNER_LOGICAL)
                .min_inner_size(RESTORE_PILL_INNER_LOGICAL, RESTORE_PILL_INNER_LOGICAL)
                .max_inner_size(RESTORE_PILL_INNER_LOGICAL, RESTORE_PILL_INNER_LOGICAL)
                .decorations(false)
                .skip_taskbar(true)
                .resizable(false)
                .transparent(true)
                .shadow(false)
                .visible(true)
                .owner(&main)
                .map_err(|e| e.to_string())?
                .build()
                .map_err(|e| e.to_string())?;
        }

        main
            .eval(JS_WALLPAPER_REPOSITION_PILL)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        send_main_window_back(&app)
    }
}

#[tauri::command]
fn bring_window_to_front(app: tauri::AppHandle) -> Result<(), String> {
    bring_main_window_forward(&app)
}

/// Recoloca a pílula no canto do calendário após o layout (janela mantém o tamanho).
#[tauri::command]
async fn reposition_restore_pill(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        let main = app
            .get_webview_window("main")
            .ok_or_else(|| "Janela principal em falta.".to_string())?;
        let pill = app
            .get_webview_window("restore-pill")
            .ok_or_else(|| "Janela da pílula em falta.".to_string())?;
        position_restore_pill(&main, &pill)?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Ok(())
    }
}

/// Chamado pela janela pequena “voltar” e equivalente a trazer à frente quando o modo wallpaper está ativo.
#[tauri::command]
fn restore_desktop_wallpaper_mode(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        restore_desktop_wallpaper_mode_internal(&app)
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = dotenvy::dotenv();
    if std::env::var("GOOGLE_OAUTH_CLIENT_ID").is_err() {
        let _ = dotenvy::from_filename("../.env");
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None::<Vec<&str>>,
        ))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_denylist(&["restore-pill"])
                .build(),
        )
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let handle = window.app_handle();
                let hide_instead = read_config_file(handle)
                    .map(|c| c.close_to_tray)
                    .unwrap_or(false);
                if hide_instead {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            undo_desktop_wallpaper_on_launch(app.handle());
            if let Err(e) = local_store::init(app.handle()) {
                eprintln!("[agenda] base local (SQLite): {e}");
            }
            if let Err(e) = setup_tray(app.handle()) {
                eprintln!("[agenda] bandeja do sistema: {e}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_config,
            save_app_config,
            get_calendar_state,
            google_calendar_sign_in,
            google_calendar_sync,
            google_calendar_disconnect,
            get_cached_calendar_events,
            google_calendar_create_event,
            google_calendar_update_event,
            google_calendar_delete_event,
            send_window_to_back,
            bring_window_to_front,
            reposition_restore_pill,
            restore_desktop_wallpaper_mode,
            autostart_set,
            autostart_is_enabled
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
