//! Entrada `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run` sem aspas no `.exe`
//! partindo caminhos com espaços (ex. `C:\Program Files\...`) — o Windows trata o primeiro
//! token como `C:\Program`, falha ou abre `cmd`/comportamentos estranhos.

use std::path::{Path, PathBuf};

use tauri::AppHandle;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
use winreg::RegKey;

const RUN_SUBKEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";

fn strip_verbatim_prefix(path: &str) -> &str {
    match path.strip_prefix(r"\\?\") {
        Some(s) if s.starts_with("UNC\\") => path,
        Some(s) => s,
        None => path,
    }
}

fn set_run_value_quoted_path(app: &AppHandle, exe: &Path) -> Result<(), String> {
    let exe = std::fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());
    let raw = exe.to_string_lossy();
    let path = strip_verbatim_prefix(&raw);
    let inner = path.replace('"', "\"\"");
    let value = format!("\"{inner}\"");

    let name = app.package_info().name.clone();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey_with_flags(RUN_SUBKEY, KEY_SET_VALUE)
        .map_err(|e| format!("autostart Run: {e}"))?
        .set_value(&name, &value)
        .map_err(|e| format!("autostart Run: {e}"))?;
    Ok(())
}

/// Reescreve o valor Run com o caminho actual do `.exe` principal entre aspas (mesmo nome de chave que o plugin autostart).
pub fn rewrite_run_value_with_quoted_exe(app: &AppHandle) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    set_run_value_quoted_path(app, &exe)
}

/// Caminho do vigia `agenda-watchdog.exe` (sidecar) na mesma pasta que o executável principal.
pub fn watchdog_exe_path() -> Result<PathBuf, String> {
    let main = std::env::current_exe().map_err(|e| e.to_string())?;
    let parent = main
        .parent()
        .ok_or_else(|| "Sem pasta do executável principal.".to_string())?;
    Ok(parent.join("agenda-watchdog.exe"))
}

/// Registo Run aponta para o vigia (deve existir `agenda-watchdog.exe` ao lado do `.exe` principal).
pub fn rewrite_run_value_with_watchdog(app: &AppHandle) -> Result<(), String> {
    let w = watchdog_exe_path()?;
    if !w.is_file() {
        return Err(
            "agenda-watchdog.exe não encontrado junto da aplicação. Usa um instalador/build que inclua o vigia."
                .to_string(),
        );
    }
    set_run_value_quoted_path(app, &w)
}

/// Se já existe entrada `Run` para esta app, regrava o caminho com aspas correctas.
///
/// **Importante:** se `autostart_use_watchdog` for `true`, o valor deve apontar para
/// `agenda-watchdog.exe`. Antes regravávamos sempre o `.exe` principal e **apagávamos** o vigia
/// no arranque seguinte.
#[cfg(not(debug_assertions))]
pub fn fix_if_autostart_entry_exists(app: &AppHandle, autostart_use_watchdog: bool) {
    let name = app.package_info().name.clone();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(run) = hkcu.open_subkey_with_flags(RUN_SUBKEY, KEY_READ) else {
        return;
    };
    if run.get_value::<String, _>(&name).is_err() {
        return;
    }
    if autostart_use_watchdog {
        if let Err(e) = rewrite_run_value_with_watchdog(app) {
            eprintln!(
                "[agenda] autostart Run: vigia indisponível ({e}); a manter/corrigir executável principal."
            );
            let _ = rewrite_run_value_with_quoted_exe(app);
        }
    } else {
        let _ = rewrite_run_value_with_quoted_exe(app);
    }
}

/// Remove a entrada de autostart desta app no registo do Windows (se existir).
pub fn remove_run_value(app: &AppHandle) -> Result<(), String> {
    let name = app.package_info().name.clone();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run = hkcu
        .open_subkey_with_flags(RUN_SUBKEY, KEY_SET_VALUE)
        .map_err(|e| format!("autostart Run: {e}"))?;
    let _ = run.delete_value(&name);
    Ok(())
}
