//! Ancoragem experimental atrás dos ícones do ambiente de trabalho (Windows).
//! Usa a camada `WorkerW` — comportamento pode mudar com atualizações do Windows.
//!
//! Após `SetParent`, o Windows costuma recolocar a janela (p.ex. noutro monitor).
//! Guardamos o retângulo em coordenadas de ecrã e voltamos a posicionar em
//! coordenadas relativas ao novo pai (ou topo-nível ao libertar).

use tauri::{Runtime, WebviewWindow};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT, DWMWA_COLOR_NONE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DEFAULT, DWMWCP_DONOTROUND, DWMWCP_ROUND,
    DWM_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::Graphics::Gdi::{
    CreateRectRgn, DeleteObject, GetMonitorInfoW, MonitorFromWindow, RedrawWindow, ScreenToClient,
    SetWindowRgn, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST, RDW_FRAME, RDW_INVALIDATE,
    RDW_UPDATENOW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetClassNameW, GetSystemMetrics, GetWindowRect, IsZoomed,
    SendMessageTimeoutW, SetParent, SetWindowPos, SMTO_NORMAL, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SWP_FRAMECHANGED, SWP_NOACTIVATE, HWND_TOP,
};

fn hwnd_or_default(r: windows::core::Result<HWND>) -> HWND {
    r.unwrap_or_default()
}

unsafe fn workerw_behind_icons() -> Option<HWND> {
    let progman = FindWindowW(w!("Progman"), None).ok()?;
    if progman.is_invalid() {
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
                    return Some(child);
                }
            }
        }
        child = hwnd_or_default(FindWindowExW(Some(progman), Some(child), None, None));
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
            let parent = workerw_behind_icons()
                .ok_or_else(|| "Camada WorkerW não encontrada.".to_string())?;
            let rect = screen_rect_for_reparent(hwnd)?;
            SetParent(hwnd, Some(parent)).map_err(|e| format!("SetParent: {e}"))?;
            move_as_child_at_screen_rect(hwnd, parent, &rect)?;
            apply_wallpaper_hwnd_chrome(hwnd, rounded_corners, show_border)?;
        } else {
            let rect = screen_rect_for_reparent(hwnd)?;
            SetParent(hwnd, None).map_err(|e| format!("SetParent: {e}"))?;
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
