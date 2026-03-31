//! Entrada `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run` sem aspas no `.exe`
//! partindo caminhos com espaços (ex. `C:\Program Files\...`) — o Windows trata o primeiro
//! token como `C:\Program`, falha ou abre `cmd`/comportamentos estranhos.

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

/// Reescreve o valor Run com o caminho atual do `.exe` entre aspas (mesmo nome de chave que o plugin autostart).
pub fn rewrite_run_value_with_quoted_exe(app: &AppHandle) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
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

/// Se já existe entrada de autostart para esta app, corrige aspas (útil após updates ou registo antigo).
#[allow(dead_code)]
pub fn fix_if_autostart_entry_exists(app: &AppHandle) {
    let name = app.package_info().name.clone();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(run) = hkcu.open_subkey_with_flags(RUN_SUBKEY, KEY_READ) else {
        return;
    };
    if run.get_value::<String, _>(&name).is_err() {
        return;
    }
    let _ = rewrite_run_value_with_quoted_exe(app);
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
