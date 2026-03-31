use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Manager, Monitor, PhysicalPosition, PhysicalSize, RunEvent, Runtime, WebviewWindow,
};

mod calendar_model;
mod google_calendar;
mod local_store;

#[cfg(windows)]
mod windows_desktop;

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

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
    /// Cantos arredondados no conteúdo da janela (CSS).
    #[serde(default = "default_true")]
    pub window_rounded_corners: bool,
    /// Contorno de 1px à volta do conteúdo (janela sem decorações nativas).
    #[serde(default = "default_true")]
    pub window_show_border: bool,
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

fn default_true() -> bool {
    true
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
            window_rounded_corners: true,
            window_show_border: true,
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
        let _ = windows_desktop::set_behind_icons(&win, false, true, true);
        clamp_webview_outer_to_work_area(&win);
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
            let _ = windows_desktop::set_behind_icons(&main, false, true, true);
            clamp_webview_outer_to_work_area(&main);
            let _ = main.eval(JS_WALLPAPER_LEAVE);
        }
    }
    undo_desktop_behind_if_needed(app);
}

/// Mesma heurística que `tauri-plugin-window-state`: algum canto do retângulo da janela está dentro do ecrã?
fn monitor_intersects_window_outer(
    m: &Monitor,
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
) -> bool {
    let left = m.position().x;
    let right = left + m.size().width as i32;
    let top = m.position().y;
    let bottom = top + m.size().height as i32;
    [
        (position.x, position.y),
        (position.x + size.width as i32, position.y),
        (position.x, position.y + size.height as i32),
        (
            position.x + size.width as i32,
            position.y + size.height as i32,
        ),
    ]
    .into_iter()
    .any(|(x, y)| x >= left && x < right && y >= top && y < bottom)
}

fn window_outer_intersects_any_monitor<R: Runtime>(
    win: &WebviewWindow<R>,
) -> tauri::Result<bool> {
    let position = win.outer_position()?;
    let size = win.outer_size()?;
    let monitors = win.available_monitors()?;
    Ok(monitors
        .iter()
        .any(|m| monitor_intersects_window_outer(m, position, size)))
}

/// Limita posição e tamanho externos à área útil do monitor que contém o centro da janela.
/// Evita que, após desmaximizar ou `SetParent`/`SetWindowPos`, a largura “vaze” para outro ecrã.
fn clamp_webview_outer_to_work_area<R: Runtime>(win: &WebviewWindow<R>) {
    #[cfg(windows)]
    if DESKTOP_WALLPAPER_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    if !win.is_visible().unwrap_or(false) {
        return;
    }
    if win.is_minimized().unwrap_or(true) {
        return;
    }
    if win.is_maximized().unwrap_or(false) {
        return;
    }
    let Ok(position) = win.outer_position() else {
        return;
    };
    let Ok(size) = win.outer_size() else {
        return;
    };
    let Ok(monitors) = win.available_monitors() else {
        return;
    };
    let Some(m) = monitors.first() else {
        return;
    };
    let cx = position.x + size.width as i32 / 2;
    let cy = position.y + size.height as i32 / 2;
    let m = monitors
        .iter()
        .find(|mon| {
            let left = mon.position().x;
            let top = mon.position().y;
            let right = left + mon.size().width as i32;
            let bottom = top + mon.size().height as i32;
            cx >= left && cx < right && cy >= top && cy < bottom
        })
        .unwrap_or(m);
    let wa = m.work_area();
    let wl = wa.position.x;
    let wt = wa.position.y;
    let wr = wl + wa.size.width as i32;
    let wb = wt + wa.size.height as i32;
    let max_w = (wr - wl).max(160) as u32;
    let max_h = (wb - wt).max(120) as u32;
    let mut left = position.x;
    let mut top = position.y;
    let mut w = size.width;
    let mut h = size.height;
    if w > max_w {
        w = max_w;
    }
    if h > max_h {
        h = max_h;
    }
    if left + w as i32 > wr {
        left = wr - w as i32;
    }
    if top + h as i32 > wb {
        top = wb - h as i32;
    }
    if left < wl {
        left = wl;
    }
    if top < wt {
        top = wt;
    }
    if left == position.x && top == position.y && w == size.width && h == size.height {
        return;
    }
    let _ = win.set_position(PhysicalPosition::new(left, top));
    let _ = win.set_size(PhysicalSize::new(w, h));
}

fn clamp_main_window_outer_to_work_area<R: Runtime>(app: &AppHandle<R>) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    clamp_webview_outer_to_work_area(&win);
}

/// Se a janela principal não intersecta nenhum ecrã (ex.: coordenadas antigas após desligar um monitor), centra.
fn clamp_main_window_to_visible_workspace<R: Runtime>(app: &AppHandle<R>) {
    #[cfg(windows)]
    if DESKTOP_WALLPAPER_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    if !win.is_visible().unwrap_or(false) {
        return;
    }
    if win.is_minimized().unwrap_or(true) {
        return;
    }
    if win.is_maximized().unwrap_or(false) {
        return;
    }
    let Ok(intersects) = window_outer_intersects_any_monitor(&win) else {
        return;
    };
    if intersects {
        return;
    }
    let _ = win.center();
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
    windows_desktop::set_behind_icons(&main, false, true, true)?;
    clamp_webview_outer_to_work_area(&main);
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
    /// Mutações à espera de rede / API (fila offline).
    #[serde(default)]
    pub pending_mutations_count: u32,
}

#[tauri::command]
fn get_calendar_state(app: tauri::AppHandle) -> CalendarState {
    let connected = google_calendar::has_refresh_token(&app);
    let pending_mutations_count = if local_store::is_ready() {
        local_store::pending_mutations_len(&app).unwrap_or(0)
    } else {
        0
    };
    CalendarState {
        source: if connected {
            "google".to_string()
        } else {
            "demo".to_string()
        },
        connected,
        db_ready: local_store::is_ready(),
        client_id_configured: google_calendar::client_id_configured(&app),
        pending_mutations_count,
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
    if let Err(e) = google_calendar::flush_pending_mutations(&app).await {
        eprintln!("[agenda] fila offline após sync: {e}");
    }
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

#[tauri::command]
async fn google_calendar_create_event(
    app: tauri::AppHandle,
    payload: calendar_model::CreateGoogleEventPayload,
) -> Result<calendar_model::CalendarEvent, String> {
    google_calendar::create_primary_calendar_event(&app, payload).await
}

#[tauri::command]
async fn google_calendar_update_event(
    app: tauri::AppHandle,
    payload: calendar_model::UpdateGoogleEventPayload,
) -> Result<calendar_model::CalendarEvent, String> {
    google_calendar::update_calendar_event(&app, payload).await
}

#[tauri::command]
async fn google_calendar_delete_event(
    app: tauri::AppHandle,
    payload: calendar_model::DeleteGoogleEventPayload,
) -> Result<(), String> {
    google_calendar::delete_calendar_event(&app, payload).await
}

/// Processa só a fila offline (sem `events.list` / sync incremental).
#[tauri::command]
async fn google_calendar_flush_offline_queue(app: tauri::AppHandle) -> Result<u32, String> {
    google_calendar::flush_pending_mutations(&app).await
}

/// Abre no explorador a pasta com SQLite e token OAuth em ficheiro (dados locais da app).
#[tauri::command]
fn open_app_local_data_folder(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("{e}"))?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy().as_ref(), Option::<&str>::None)
        .map_err(|e| e.to_string())
}

/// Abre a pasta de configuração (`config.json`, estado da janela, opcionalmente `google_oauth_client_id.txt`).
#[tauri::command]
fn open_app_config_folder(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = app.path().app_config_dir().map_err(|e| format!("{e}"))?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy().as_ref(), Option::<&str>::None)
        .map_err(|e| e.to_string())
}

/// Remove o ficheiro de estado da janela e centra a janela principal com o tamanho inicial (380×520).
#[tauri::command]
fn reset_saved_window_layout(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_window_state::AppHandleExt;
    let cfg_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let state_path = cfg_dir.join(app.filename());
    if state_path.is_file() {
        fs::remove_file(&state_path).map_err(|e| e.to_string())?;
    }
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "Janela principal em falta.".to_string())?;
    win
        .set_size(tauri::LogicalSize::new(380.0, 520.0))
        .map_err(|e| e.to_string())?;
    win.center().map_err(|e| e.to_string())?;
    Ok(())
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
        let cfg = read_config_file(&app).unwrap_or_default();
        windows_desktop::set_behind_icons(
            &main,
            true,
            cfg.window_rounded_corners,
            cfg.window_show_border,
        )?;
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

#[tauri::command]
fn window_minimize(app: tauri::AppHandle) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "Janela principal em falta.".to_string())?
        .minimize()
        .map_err(|e| e.to_string())
}

/// Pedido de fecho (respeita «fechar → bandeja» em `on_window_event`).
#[tauri::command]
fn window_close_main(app: tauri::AppHandle) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "Janela principal em falta.".to_string())?
        .close()
        .map_err(|e| e.to_string())
}

/// Maximiza ou restaura a janela (área útil do ecrã, não fullscreen exclusivo da API).
#[tauri::command]
fn window_toggle_maximized(app: tauri::AppHandle) -> Result<bool, String> {
    let w = app
        .get_webview_window("main")
        .ok_or_else(|| "Janela principal em falta.".to_string())?;
    if w.is_maximized().map_err(|e| e.to_string())? {
        w.unmaximize().map_err(|e| e.to_string())?;
        clamp_webview_outer_to_work_area(&w);
        Ok(false)
    } else {
        w.maximize().map_err(|e| e.to_string())?;
        Ok(true)
    }
}

#[tauri::command]
fn window_is_maximized(app: tauri::AppHandle) -> Result<bool, String> {
    app.get_webview_window("main")
        .ok_or_else(|| "Janela principal em falta.".to_string())?
        .is_maximized()
        .map_err(|e| e.to_string())
}

/// Garante que a janela principal não ficou fora de todos os monitores (útil após mudar ecrãs).
#[tauri::command]
fn ensure_main_window_on_screen(app: tauri::AppHandle) -> Result<(), String> {
    clamp_main_window_outer_to_work_area(&app);
    clamp_main_window_to_visible_workspace(&app);
    Ok(())
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

/// Atualiza cantos/borda DWM no modo «atrás dos ícones» (o CSS não controla o contorno do HWND).
#[tauri::command]
fn sync_desktop_wallpaper_dwm_prefs(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        if !DESKTOP_WALLPAPER_ACTIVE.load(Ordering::SeqCst) {
            return Ok(());
        }
        let cfg = read_config_file(&app).map_err(|e| e.to_string())?;
        let main = app
            .get_webview_window("main")
            .ok_or_else(|| "Janela principal em falta.".to_string())?;
        windows_desktop::apply_wallpaper_dwm_style(
            &main,
            cfg.window_rounded_corners,
            cfg.window_show_border,
        )?;
    }
    Ok(())
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
            let h = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(150));
                let handle = h.clone();
                let _ = h.run_on_main_thread(move || {
                    clamp_main_window_to_visible_workspace(&handle);
                });
            });
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
            google_calendar_flush_offline_queue,
            open_app_local_data_folder,
            open_app_config_folder,
            reset_saved_window_layout,
            send_window_to_back,
            bring_window_to_front,
            window_minimize,
            window_close_main,
            window_toggle_maximized,
            window_is_maximized,
            ensure_main_window_on_screen,
            reposition_restore_pill,
            restore_desktop_wallpaper_mode,
            sync_desktop_wallpaper_dwm_prefs,
            autostart_set,
            autostart_is_enabled
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, RunEvent::Resumed) {
                clamp_main_window_to_visible_workspace(app);
            }
        });
}
