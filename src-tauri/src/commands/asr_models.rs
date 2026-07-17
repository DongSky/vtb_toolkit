//! Whisper model management commands.

use tauri::{AppHandle, Emitter};

pub const EVENT_MODEL_PROGRESS: &str = "model://progress";

#[derive(serde::Serialize, Clone)]
struct ModelProgress {
    name: String,
    downloaded: u64,
    total: Option<u64>,
}

#[tauri::command]
pub async fn asr_models() -> Result<Vec<vtb_asr::models::ModelStatus>, String> {
    Ok(vtb_asr::models::list_models())
}

#[tauri::command]
pub async fn asr_model_download(app: AppHandle, name: String) -> Result<String, String> {
    let n = name.clone();
    let mut last_emit = std::time::Instant::now();
    let path = vtb_asr::models::download_model(&name, move |downloaded, total| {
        // Throttle progress events to ~4/s.
        if last_emit.elapsed() >= std::time::Duration::from_millis(250) {
            last_emit = std::time::Instant::now();
            let _ = app.emit(
                EVENT_MODEL_PROGRESS,
                ModelProgress {
                    name: n.clone(),
                    downloaded,
                    total,
                },
            );
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}
