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
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DEFAULT, DWMWCP_ROUND,
    DWM_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetClassNameW, GetSystemMetrics, GetWindowRect, SendMessageTimeoutW,
    SetParent, SetWindowPos, SMTO_NORMAL, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SWP_FRAMECHANGED, SWP_NOACTIVATE, HWND_TOP,
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

/// Após `SetParent` para o WorkerW o DWM costuma tratar a janela como retangular; pedimos cantos redondos (Win11+).
unsafe fn set_dwm_corner_preference(hwnd: HWND, pref: DWM_WINDOW_CORNER_PREFERENCE) {
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE,
        &pref as *const DWM_WINDOW_CORNER_PREFERENCE as *const _,
        std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
    );
}

pub fn set_behind_icons<R: Runtime>(window: &WebviewWindow<R>, enable: bool) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    unsafe {
        if enable {
            let parent = workerw_behind_icons()
                .ok_or_else(|| "Camada WorkerW não encontrada.".to_string())?;
            let rect = outer_rect_screen(hwnd)?;
            SetParent(hwnd, Some(parent)).map_err(|e| format!("SetParent: {e}"))?;
            move_as_child_at_screen_rect(hwnd, parent, &rect)?;
            set_dwm_corner_preference(hwnd, DWMWCP_ROUND);
        } else {
            let rect = outer_rect_screen(hwnd)?;
            SetParent(hwnd, None).map_err(|e| format!("SetParent: {e}"))?;
            move_top_level_at_screen_rect(hwnd, &rect)?;
            set_dwm_corner_preference(hwnd, DWMWCP_DEFAULT);
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
