//! App settings persistence (JSON file in the app config dir) and named
//! secrets (OS keychain). Ordinary preferences live in plaintext JSON;
//! anything sensitive goes through [`vtb_account::Secrets`].

use std::path::{Path, PathBuf};
use tauri::Manager;
use vtb_account::Secrets;

fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

/// Read the whole settings object (for other command modules).
pub fn read_settings(app: &tauri::AppHandle) -> serde_json::Value {
    settings_path(app)
        .map(|p| load_settings(&p))
        .unwrap_or_else(|_| serde_json::json!({}))
}

/// Load the whole settings object (empty object when absent/corrupt).
pub fn load_settings(path: &Path) -> serde_json::Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(|v: &serde_json::Value| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// Set one key and persist atomically (write temp + rename).
pub fn save_key(path: &Path, key: &str, value: serde_json::Value) -> Result<(), String> {
    let mut settings = load_settings(path);
    settings[key] = value;
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn config_load(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    Ok(load_settings(&settings_path(&app)?))
}

#[tauri::command]
pub async fn config_set(
    app: tauri::AppHandle,
    key: String,
    value: serde_json::Value,
) -> Result<(), String> {
    save_key(&settings_path(&app)?, &key, value)
}

const ALLOWED_SECRETS: &[&str] = &["llm-api-key"];

fn check_secret_name(name: &str) -> Result<(), String> {
    if ALLOWED_SECRETS.contains(&name) {
        Ok(())
    } else {
        Err(format!("unknown secret name: {name}"))
    }
}

#[tauri::command]
pub async fn secret_set(name: String, value: String) -> Result<(), String> {
    check_secret_name(&name)?;
    if value.is_empty() {
        Secrets::delete(&name).map_err(|e| e.to_string())
    } else {
        Secrets::set(&name, &value).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn secret_exists(name: String) -> Result<bool, String> {
    check_secret_name(&name)?;
    Ok(Secrets::get(&name).map_err(|e| e.to_string())?.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip_and_merge() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");

        assert_eq!(load_settings(&path), serde_json::json!({}));

        save_key(&path, "room_id", serde_json::json!(24158116)).unwrap();
        save_key(&path, "output_dir", serde_json::json!("/rec")).unwrap();
        let v = load_settings(&path);
        assert_eq!(v["room_id"], 24158116);
        assert_eq!(v["output_dir"], "/rec");

        // Overwrite one key, keep the other.
        save_key(&path, "room_id", serde_json::json!(320)).unwrap();
        let v = load_settings(&path);
        assert_eq!(v["room_id"], 320);
        assert_eq!(v["output_dir"], "/rec");
    }

    #[test]
    fn corrupt_settings_reset_to_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{broken").unwrap();
        assert_eq!(load_settings(&path), serde_json::json!({}));
        // And saving over the corrupt file works.
        save_key(&path, "k", serde_json::json!(1)).unwrap();
        assert_eq!(load_settings(&path)["k"], 1);
    }

    #[test]
    fn secret_names_are_allowlisted() {
        assert!(check_secret_name("llm-api-key").is_ok());
        assert!(check_secret_name("../evil").is_err());
    }
}
