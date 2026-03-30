use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    /// "widget" | "app"
    pub view_mode: String,
    /// "light" | "dark" | "system"
    pub theme: String,
    #[serde(default = "default_opacity")]
    pub widget_opacity: f64,
}

fn default_opacity() -> f64 {
    1.0
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            view_mode: "widget".to_string(),
            theme: "dark".to_string(),
            widget_opacity: 1.0,
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

#[tauri::command]
fn get_app_config(app: tauri::AppHandle) -> Result<AppConfig, String> {
    let path = config_path(&app)?;
    if path.exists() {
        let s = fs::read_to_string(&path).map_err(|e| format!("read config: {e}"))?;
        serde_json::from_str(&s).map_err(|e| format!("parse config: {e}"))
    } else {
        Ok(AppConfig::default())
    }
}

#[tauri::command]
fn save_app_config(app: tauri::AppHandle, config: AppConfig) -> Result<(), String> {
    let path = config_path(&app)?;
    let s =
        serde_json::to_string_pretty(&config).map_err(|e| format!("serialize config: {e}"))?;
    fs::write(&path, s).map_err(|e| format!("write config: {e}"))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .invoke_handler(tauri::generate_handler![get_app_config, save_app_config])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
