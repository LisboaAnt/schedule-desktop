//! Ancoragem experimental atrás dos ícones do ambiente de trabalho (Windows).
//! Usa a camada `WorkerW` — comportamento pode mudar com atualizações do Windows.
//!
//! Após `SetParent`, o Windows costuma recolocar a janela (p.ex. noutro monitor).
//! Guardamos o retângulo em coordenadas de ecrã e voltamos a posicionar em
//! coordenadas relativas ao novo pai (ou topo-nível ao libertar).

use std::sync::{Mutex, OnceLock};

use crate::workerw_log;
use tauri::{Runtime, WebviewWindow};
use windows::core::w;
use windows::Win32::Foundation::{GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Foundation::ERROR_CLASS_ALREADY_EXISTS;
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT, DWMWA_COLOR_NONE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DEFAULT, DWMWCP_DONOTROUND, DWMWCP_ROUND,
    DWM_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::Graphics::Gdi::{
    CreateRectRgn, DeleteObject, GetMonitorInfoW, MonitorFromWindow, RedrawWindow, ScreenToClient,
    SetWindowRgn, HBRUSH, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST, RDW_FRAME, RDW_INVALIDATE,
    RDW_UPDATENOW,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowExW, FindWindowW, GetAncestor,
    GetClassNameW, GetMessageW, GetSystemMetrics, GetWindowRect, HCURSOR, HICON, IsIconic, IsWindow,
    IsWindowVisible, IsZoomed, RegisterClassW, SendMessageTimeoutW, SetParent, SetWindowPos,
    ShowWindow, TranslateMessage, MSG, SMTO_NORMAL, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SPI_SETDESKWALLPAPER, SWP_FRAMECHANGED, SWP_NOACTIVATE,
    SW_HIDE, SW_RESTORE, SW_SHOWNOACTIVATE, WM_SETTINGCHANGE,
    WNDCLASSW, WNDCLASS_STYLES, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP, GA_PARENT, HWND_TOP,
};
use windows::core::PCWSTR;

/// Debug do modo WorkerW: `debug` build ou `AGENDA_WORKERW_DEBUG=1`.
pub(crate) fn workerw_debug_enabled() -> bool {
    cfg!(debug_assertions)
        || std::env::var("AGENDA_WORKERW_DEBUG")
            .ok()
            .as_deref()
            == Some("1")
}

fn hwnd_usize(hwnd: HWND) -> usize {
    hwnd.0 as usize
}

fn is_window_alive(hwnd: HWND) -> bool {
    if hwnd.is_invalid() {
        return false;
    }
    unsafe { IsWindow(Some(hwnd)).as_bool() }
}

/// Último estilo DWM/região aplicado ao HWND no modo wallpaper (evita refazer tudo quando só o WorkerW muda).
static LAST_WALLPAPER_CHROME: Mutex<Option<(bool, bool)>> = Mutex::new(None);

fn record_wallpaper_chrome_applied(rounded_corners: bool, show_border: bool) {
    if let Ok(mut g) = LAST_WALLPAPER_CHROME.lock() {
        *g = Some((rounded_corners, show_border));
    }
}

/// Chamado ao sair do modo wallpaper ou quando o utilizador altera cantos/borda nas definições.
pub fn clear_wallpaper_chrome_cache() {
    if let Ok(mut g) = LAST_WALLPAPER_CHROME.lock() {
        *g = None;
    }
}

/// Pai imediato na hierarquia Win32.
///
/// Não usar `GetParent` do crate `windows` 0.61 para isto: quando o Win32 devolve NULL,
/// o binding mapeia para `Err` e `unwrap_or_default()` vira HWND 0 — mascarando janelas
/// ancoradas e forçando reancoragens em loop. `GetAncestor(..., GA_PARENT)` devolve o
/// HWND directamente (NULL = sem pai).
fn get_parent_hwnd(hwnd: HWND) -> HWND {
    unsafe { GetAncestor(hwnd, GA_PARENT) }
}

/// `true` se Win32 indica que convém `show`/`unminimize` antes de `SetParent` / chrome.
pub fn hwnd_needs_show_unminimize(hwnd: HWND) -> bool {
    unsafe { IsIconic(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() }
}

/// O que fazer na reancoragem periódica — evita `SetParent` completo quando só a visibilidade oscila (ex.: mudança de wallpaper).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallpaperHealKind {
    /// Já ancorado ao WorkerW actual e visível.
    None,
    /// Pai correcto; só recuperar visibilidade (sem `SetParent` / DWM completo).
    LightVisibility,
    /// Reparent ou WorkerW em falta — caminho completo `set_behind_icons`.
    FullReparent,
}

pub fn classify_wallpaper_heal<R: Runtime>(
    window: &WebviewWindow<R>,
) -> Result<WallpaperHealKind, String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    unsafe {
        if !is_window_alive(hwnd) {
            if workerw_debug_enabled() {
                eprintln!("[agenda] workerw: classify main HWND inválido ou destruído");
            }
            return Ok(WallpaperHealKind::FullReparent);
        }
        let parent = get_parent_hwnd(hwnd);
        if !parent.is_invalid() && !is_window_alive(parent) {
            if workerw_debug_enabled() {
                eprintln!(
                    "[agenda] workerw: classify orphan parent hwnd={}",
                    hwnd_usize(parent)
                );
            }
            return Ok(WallpaperHealKind::FullReparent);
        }
        let Some(workerw) = workerw_behind_icons() else {
            return Ok(WallpaperHealKind::FullReparent);
        };
        if !is_window_alive(workerw) {
            if workerw_debug_enabled() {
                eprintln!("[agenda] workerw: classify WorkerW inválido após descoberta");
            }
            return Ok(WallpaperHealKind::FullReparent);
        }
        if parent != workerw {
            return Ok(WallpaperHealKind::FullReparent);
        }
        if IsIconic(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() {
            return Ok(WallpaperHealKind::LightVisibility);
        }
        Ok(WallpaperHealKind::None)
    }
}

/// `ShowWindow` mínimo quando o pai já é o WorkerW (não repetir `SetParent`).
pub fn restore_wallpaper_visibility_hwnd(hwnd: HWND) {
    unsafe {
        ensure_wallpaper_window_visible(hwnd);
    }
}

fn hwnd_or_default(r: windows::core::Result<HWND>) -> HWND {
    r.unwrap_or_default()
}

unsafe fn workerw_behind_icons() -> Option<HWND> {
    let progman = FindWindowW(w!("Progman"), None).ok()?;
    if progman.is_invalid() {
        if workerw_debug_enabled() {
            eprintln!("[agenda] workerw: Progman não encontrado ou HWND inválido");
        }
        return None;
    }

    let mut smto = 0usize;
    let _ = SendMessageTimeoutW(
        progman,
        0x052C,
        WPARAM(0),
        LPARAM(0),
        SMTO_NORMAL,
        1000,
        Some(&mut smto),
    );

    let mut child = hwnd_or_default(FindWindowExW(Some(progman), None, None, None));
    while !child.is_invalid() {
        let def = hwnd_or_default(FindWindowExW(
            Some(child),
            None,
            w!("SHELLDLL_DefView"),
            None,
        ));
        if def.is_invalid() {
            let mut buf = [0u16; 64];
            let n = GetClassNameW(child, &mut buf);
            if n > 0 {
                let name = String::from_utf16_lossy(&buf[..n as usize]);
                if name == "WorkerW" {
                    if workerw_debug_enabled() {
                        eprintln!(
                            "[agenda] workerw: WorkerW encontrado hwnd={}",
                            hwnd_usize(child)
                        );
                    }
                    return Some(child);
                }
            }
        }
        child = hwnd_or_default(FindWindowExW(Some(progman), Some(child), None, None));
    }
    if workerw_debug_enabled() {
        eprintln!("[agenda] workerw: WorkerW não encontrado (varredura Progman terminou)");
    }
    None
}

unsafe fn outer_rect_screen(hwnd: HWND) -> Result<RECT, String> {
    let mut r = RECT::default();
    GetWindowRect(hwnd, &mut r).map_err(|e| format!("GetWindowRect: {e}"))?;
    Ok(r)
}

/// Com janela maximizada, `GetWindowRect` pode não coincidir com um único monitor (multi‑ecrã / DPI).
/// Usamos a área de trabalho do monitor do HWND para posicionar como filho do WorkerW.
unsafe fn work_area_screen_rect_for_window(hwnd: HWND) -> Result<RECT, String> {
    let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    if hmon.is_invalid() {
        return outer_rect_screen(hwnd);
    }
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(hmon, &mut mi).as_bool() {
        return outer_rect_screen(hwnd);
    }
    Ok(mi.rcWork)
}

unsafe fn screen_rect_for_reparent(hwnd: HWND) -> Result<RECT, String> {
    if IsZoomed(hwnd).as_bool() {
        work_area_screen_rect_for_window(hwnd)
    } else {
        outer_rect_screen(hwnd)
    }
}

unsafe fn move_as_child_at_screen_rect(
    child: HWND,
    parent: HWND,
    screen: &RECT,
) -> Result<(), String> {
    let w = screen.right - screen.left;
    let h = screen.bottom - screen.top;
    let mut pt = POINT {
        x: screen.left,
        y: screen.top,
    };
    if !ScreenToClient(parent, &mut pt).as_bool() {
        return Err("ScreenToClient falhou.".to_string());
    }
    SetWindowPos(
        child,
        Some(HWND_TOP),
        pt.x,
        pt.y,
        w,
        h,
        SWP_NOACTIVATE | SWP_FRAMECHANGED,
    )
    .map_err(|e| format!("SetWindowPos (filho): {e}"))?;
    Ok(())
}

unsafe fn move_top_level_at_screen_rect(hwnd: HWND, screen: &RECT) -> Result<(), String> {
    let w = screen.right - screen.left;
    let h = screen.bottom - screen.top;
    SetWindowPos(
        hwnd,
        Some(HWND_TOP),
        screen.left,
        screen.top,
        w,
        h,
        SWP_NOACTIVATE | SWP_FRAMECHANGED,
    )
    .map_err(|e| format!("SetWindowPos (topo): {e}"))?;
    Ok(())
}

unsafe fn set_dwm_corner_preference(hwnd: HWND, pref: DWM_WINDOW_CORNER_PREFERENCE) {
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE,
        &pref as *const DWM_WINDOW_CORNER_PREFERENCE as *const _,
        std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
    );
}

/// Contorno desenhado pelo DWM (Win11+). `DWMWA_COLOR_NONE` remove; `DWMWA_COLOR_DEFAULT` repõe o sistema.
unsafe fn set_dwm_border_visible(hwnd: HWND, show: bool) {
    let color: u32 = if show {
        DWMWA_COLOR_DEFAULT
    } else {
        DWMWA_COLOR_NONE
    };
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_BORDER_COLOR,
        &color as *const u32 as *const _,
        std::mem::size_of::<u32>() as u32,
    );
}

/// O DWM costuma ignorar `DWMWCP_DONOTROUND` em filhos do WorkerW; uma região rectangular força cantos retos.
unsafe fn apply_window_region_for_corners(hwnd: HWND, rounded_corners: bool) -> Result<(), String> {
    if rounded_corners {
        let _ = SetWindowRgn(hwnd, None, true);
        return Ok(());
    }
    let r = outer_rect_screen(hwnd)?;
    let w = r.right - r.left;
    let h = r.bottom - r.top;
    if w <= 0 || h <= 0 {
        return Err("Dimensão da janela inválida.".to_string());
    }
    let hrgn = CreateRectRgn(0, 0, w, h);
    if hrgn.is_invalid() {
        return Err("CreateRectRgn falhou.".to_string());
    }
    let ok = SetWindowRgn(hwnd, Some(hrgn), true);
    if ok == 0 {
        let _ = DeleteObject(HGDIOBJ(hrgn.0));
        return Err("SetWindowRgn falhou.".to_string());
    }
    Ok(())
}

/// Garante que a janela não fica minimizada nem oculta após `SetParent` (Explorer pode alterar estado).
unsafe fn ensure_wallpaper_window_visible(hwnd: HWND) {
    if IsIconic(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_RESTORE);
    }
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
}

unsafe fn redraw_window_frame(hwnd: HWND) {
    let _ = RedrawWindow(
        Some(hwnd),
        None,
        None,
        RDW_INVALIDATE | RDW_FRAME | RDW_UPDATENOW,
    );
}

/// DWM + região da janela (WorkerW) + redesenho.
unsafe fn apply_wallpaper_hwnd_chrome(
    hwnd: HWND,
    rounded_corners: bool,
    show_border: bool,
) -> Result<(), String> {
    set_dwm_corner_preference(
        hwnd,
        if rounded_corners {
            DWMWCP_ROUND
        } else {
            DWMWCP_DONOTROUND
        },
    );
    set_dwm_border_visible(hwnd, show_border);
    apply_window_region_for_corners(hwnd, rounded_corners)?;
    redraw_window_frame(hwnd);
    record_wallpaper_chrome_applied(rounded_corners, show_border);
    Ok(())
}

/// Cantos e borda ao nível do DWM + região (o CSS não molda o HWND no modo wallpaper).
pub fn apply_wallpaper_dwm_style<R: Runtime>(
    window: &WebviewWindow<R>,
    rounded_corners: bool,
    show_border: bool,
) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    unsafe {
        apply_wallpaper_hwnd_chrome(hwnd, rounded_corners, show_border)?;
    }
    Ok(())
}

/// Após ancorar ao WorkerW, força o HWND a coincidir com a área de trabalho (`rcWork`) do monitor
/// onde a janela se encontra (equivalente a maximizado na área útil).
///
/// `set_behind_icons` usa `GetWindowRect` quando a janela **não** está maximizada; o estado
/// persistido pode restaurar um rect intermédio, deixando margens visíveis após relançar.
pub fn snap_wallpaper_window_to_monitor_work_area<R: Runtime>(
    window: &WebviewWindow<R>,
    rounded_corners: bool,
    show_border: bool,
) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    unsafe {
        let parent = get_parent_hwnd(hwnd);
        if parent.is_invalid() {
            return Err("HWND principal sem pai (não ancorado ao WorkerW).".to_string());
        }
        let rect = work_area_screen_rect_for_window(hwnd)?;
        move_as_child_at_screen_rect(hwnd, parent, &rect)?;
        ensure_wallpaper_window_visible(hwnd);
        apply_wallpaper_hwnd_chrome(hwnd, rounded_corners, show_border)?;
    }
    Ok(())
}

fn reset_top_level_dwm(hwnd: HWND) {
    unsafe {
        let _ = SetWindowRgn(hwnd, None, true);
        set_dwm_corner_preference(hwnd, DWMWCP_DEFAULT);
        set_dwm_border_visible(hwnd, true);
        redraw_window_frame(hwnd);
    }
}

pub fn set_behind_icons<R: Runtime>(
    window: &WebviewWindow<R>,
    enable: bool,
    rounded_corners: bool,
    show_border: bool,
) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    unsafe {
        if enable {
            if !is_window_alive(hwnd) {
                return Err("HWND principal inválido antes de ancorar ao WorkerW.".to_string());
            }
            let parent_before = get_parent_hwnd(hwnd);
            if workerw_debug_enabled() {
                eprintln!(
                    "[agenda] workerw set_behind_icons enable=true hwnd={} parent_before={}",
                    hwnd_usize(hwnd),
                    hwnd_usize(parent_before)
                );
            }
            let parent = workerw_behind_icons()
                .ok_or_else(|| "Camada WorkerW não encontrada.".to_string())?;
            if !is_window_alive(parent) {
                return Err("WorkerW inválido (janela destruída).".to_string());
            }
            if workerw_debug_enabled() {
                eprintln!(
                    "[agenda] workerw: expected_parent(WorkerW)={}",
                    hwnd_usize(parent)
                );
            }
            // Já é filho deste WorkerW: não repetir `SetParent` nem refazer DWM/região (reduz flicker e risco no WebView2).
            if parent_before == parent {
                if workerw_debug_enabled() {
                    eprintln!("[agenda] workerw: set_behind_icons skip_SetParent same_parent resync_only");
                }
                workerw_log::append_line(&format!(
                    "set_behind_icons same_parent_resync hwnd={} workerw={}",
                    hwnd_usize(hwnd),
                    hwnd_usize(parent)
                ));
                let rect = screen_rect_for_reparent(hwnd)?;
                move_as_child_at_screen_rect(hwnd, parent, &rect)?;
                ensure_wallpaper_window_visible(hwnd);
                return Ok(());
            }
            let skip_dwm_chrome = parent_before != parent
                && LAST_WALLPAPER_CHROME
                    .lock()
                    .ok()
                    .and_then(|g| *g)
                    .map(|(lr, lb)| lr == rounded_corners && lb == show_border)
                    .unwrap_or(false);
            if skip_dwm_chrome && workerw_debug_enabled() {
                eprintln!(
                    "[agenda] workerw: set_behind_icons skip_DWM chrome (WorkerW mudou, mesmo estilo DWM)"
                );
            }
            let rect = screen_rect_for_reparent(hwnd)?;
            workerw_log::append_line(&format!(
                "set_behind_icons pre_SetParent hwnd={} parent_before={} workerw={}",
                hwnd_usize(hwnd),
                hwnd_usize(parent_before),
                hwnd_usize(parent)
            ));
            SetParent(hwnd, Some(parent)).map_err(|e| {
                let code = GetLastError().0;
                format!("SetParent: {e} (GetLastError={code})")
            })?;
            let parent_after = get_parent_hwnd(hwnd);
            workerw_log::append_line(&format!(
                "set_behind_icons post_SetParent parent_after={} expected={} match={}",
                hwnd_usize(parent_after),
                hwnd_usize(parent),
                parent_after == parent
            ));
            if workerw_debug_enabled() {
                let ok = parent_after == parent;
                eprintln!(
                    "[agenda] workerw: após SetParent parent_after={} match_expected={}",
                    hwnd_usize(parent_after),
                    ok
                );
            }
            if !is_window_alive(parent) || !is_window_alive(hwnd) {
                return Err("HWND inválido após SetParent (WorkerW ou janela principal).".to_string());
            }
            if parent_after != parent {
                return Err("SetParent não fixou o pai esperado (WorkerW).".to_string());
            }
            move_as_child_at_screen_rect(hwnd, parent, &rect)?;
            if workerw_debug_enabled() {
                eprintln!(
                    "[agenda] workerw: após move_as_child IsIconic={}",
                    IsIconic(hwnd).as_bool()
                );
            }
            if skip_dwm_chrome {
                record_wallpaper_chrome_applied(rounded_corners, show_border);
            } else {
                apply_wallpaper_hwnd_chrome(hwnd, rounded_corners, show_border)?;
            }
            ensure_wallpaper_window_visible(hwnd);
            if workerw_debug_enabled() {
                eprintln!(
                    "[agenda] workerw: após ensure_visible IsIconic={}",
                    IsIconic(hwnd).as_bool()
                );
            }
        } else {
            clear_wallpaper_chrome_cache();
            let parent_before = get_parent_hwnd(hwnd);
            if workerw_debug_enabled() {
                eprintln!(
                    "[agenda] workerw set_behind_icons enable=false hwnd={} parent_before={}",
                    hwnd_usize(hwnd),
                    hwnd_usize(parent_before)
                );
            }
            let rect = screen_rect_for_reparent(hwnd)?;
            workerw_log::append_line(&format!(
                "set_behind_icons pre_SetParent(None) hwnd={} parent_before={}",
                hwnd_usize(hwnd),
                hwnd_usize(parent_before)
            ));
            SetParent(hwnd, None).map_err(|e| {
                let code = GetLastError().0;
                format!("SetParent: {e} (GetLastError={code})")
            })?;
            let parent_after = get_parent_hwnd(hwnd);
            workerw_log::append_line(&format!(
                "set_behind_icons post_SetParent(None) parent_after={}",
                hwnd_usize(parent_after)
            ));
            if workerw_debug_enabled() {
                eprintln!(
                    "[agenda] workerw: após SetParent(None) parent_after={}",
                    hwnd_usize(parent_after)
                );
            }
            move_top_level_at_screen_rect(hwnd, &rect)?;
            reset_top_level_dwm(hwnd);
        }
    }
    Ok(())
}

/// Canto inferior direito da janela principal (coordenadas de ecrã), ~4 px para dentro da borda.
pub fn physical_position_for_pill_beside_main(
    main_hwnd: HWND,
    pill_w: i32,
    pill_h: i32,
) -> Result<(i32, i32), String> {
    unsafe {
        let r = outer_rect_screen(main_hwnd)?;
        let inset = 9i32;
        let mut x = r.right - pill_w - inset;
        let mut y = r.bottom - pill_h - inset;

        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        if x < r.left {
            x = r.left;
        }
        if y < r.top {
            y = r.top;
        }
        if x < vx {
            x = vx;
        }
        if y < vy {
            y = vy;
        }
        if x + pill_w > vx + vw {
            x = (vx + vw - pill_w).max(vx);
        }
        if y + pill_h > vy + vh {
            y = (vy + vh - pill_h).max(vy);
        }

        Ok((x, y))
    }
}

static WALLPAPER_SETTING_CHANGE_CB: OnceLock<Box<dyn Fn() + Send + Sync + 'static>> =
    OnceLock::new();

/// Janela top-level oculta que recebe `WM_SETTINGCHANGE` (ex.: `SPI_SETDESKWALLPAPER`) mais cedo que o watchdog.
/// Executa `on_setting_change` na thread do loop; o callback deve ser barato (ex.: agendar reancoragem debounced).
pub fn spawn_wallpaper_setting_listener(on_setting_change: impl Fn() + Send + Sync + 'static) {
    if WALLPAPER_SETTING_CHANGE_CB
        .set(Box::new(on_setting_change))
        .is_err()
    {
        return;
    }
    std::thread::spawn(|| unsafe {
        wallpaper_setting_listener_thread_main();
    });
}

/// Win11 envia frequentemente `WM_SETTINGCHANGE` com `wParam=0` ao mudar wallpaper (rajada); não confiar só em `SPI_SETDESKWALLPAPER`.
/// Por defeito agenda reancoragem (debounce em `lib.rs`); define **`AGENDA_WORKERW_WMSC0=0`** para desligar este ramo.
fn wms_c0_triggers_reanchor() -> bool {
    std::env::var("AGENDA_WORKERW_WMSC0")
        .ok()
        .as_deref()
        != Some("0")
}

unsafe extern "system" fn wallpaper_setting_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    if msg == WM_SETTINGCHANGE {
        let wp = wparam.0 as u32;
        workerw_log::append_line(&format!("WM_SETTINGCHANGE recv wparam={wp}"));
        let mut schedule = false;
        if wp == SPI_SETDESKWALLPAPER.0 {
            workerw_log::append_line(
                "WM_SETTINGCHANGE SPI_SETDESKWALLPAPER (antes de schedule debounce)",
            );
            schedule = true;
        } else if wp == 0 && wms_c0_triggers_reanchor() {
            workerw_log::append_line(
                "WM_SETTINGCHANGE wparam=0 (schedule debounce; desligar com AGENDA_WORKERW_WMSC0=0)",
            );
            schedule = true;
        }
        if schedule {
            if let Some(cb) = WALLPAPER_SETTING_CHANGE_CB.get() {
                cb();
            }
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, _lparam) }
}

unsafe fn wallpaper_setting_listener_thread_main() {
    let class_name = w!("AgendaWallpaperSettingCb");
    let hmodule = match GetModuleHandleW(None) {
        Ok(m) => m,
        Err(_) => return,
    };
    let hinstance = HINSTANCE(hmodule.0);
    let wc = WNDCLASSW {
        style: WNDCLASS_STYLES(0),
        lpfnWndProc: Some(wallpaper_setting_wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: HICON::default(),
        hCursor: HCURSOR::default(),
        hbrBackground: HBRUSH::default(),
        lpszMenuName: PCWSTR::null(),
        lpszClassName: class_name,
    };
    if RegisterClassW(&wc) == 0 {
        let e = GetLastError();
        if e != ERROR_CLASS_ALREADY_EXISTS {
            if workerw_debug_enabled() {
                eprintln!(
                    "[agenda] workerw: RegisterClassW listener falhou: {:?}",
                    e
                );
            }
            return;
        }
    }
    // Top-level (parent NULL), não HWND_MESSAGE — caso contrário BroadcastSystemMessage / WM_SETTINGCHANGE
    // com mudança de wallpaper não chega ao listener (comportamento documentado do Win32).
    let ex_style = WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
    let hwnd = match CreateWindowExW(
        ex_style,
        class_name,
        class_name,
        WS_POPUP,
        -32000,
        -32000,
        1,
        1,
        None,
        None,
        Some(hinstance),
        None,
    ) {
        Ok(h) => h,
        Err(e) => {
            if workerw_debug_enabled() {
                eprintln!("[agenda] workerw: CreateWindowExW listener: {e}");
            }
            return;
        }
    };
    if hwnd.is_invalid() {
        return;
    }
    let _ = ShowWindow(hwnd, SW_HIDE);
    workerw_log::append_line(&format!(
        "wallpaper_listener ready top_level_hidden hwnd={}",
        hwnd_usize(hwnd)
    ));
    let mut msg = MSG::default();
    loop {
        let r = GetMessageW(&mut msg, None, 0, 0);
        if r.as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        } else {
            break;
        }
    }
}
