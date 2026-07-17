//! Streamer translation profile management (glossary + reference pairs).

use tauri::Manager;
use vtb_translate::StreamerProfile;

fn profiles_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn safe_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

#[tauri::command]
pub async fn profile_save(
    app: tauri::AppHandle,
    name: String,
    profile: StreamerProfile,
) -> Result<String, String> {
    let path = profiles_dir(&app)?.join(format!("{}.json", safe_name(&name)));
    profile.save(&path).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn profile_load(
    app: tauri::AppHandle,
    name: String,
) -> Result<StreamerProfile, String> {
    let path = profiles_dir(&app)?.join(format!("{}.json", safe_name(&name)));
    StreamerProfile::load(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn profile_list(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let dir = profiles_dir(&app)?;
    let mut names = vec![];
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if let Some(name) = entry
            .path()
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
        {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                names.push(name);
            }
        }
    }
    names.sort();
    Ok(names)
}
