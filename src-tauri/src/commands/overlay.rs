//! OBS overlay server commands.

use crate::state::AppState;
use tauri::{AppHandle, Manager, State};

#[derive(serde::Serialize)]
pub struct OverlayStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub danmaku_url: Option<String>,
    pub subtitle_url: Option<String>,
    /// Connected overlay pages (WS clients).
    pub clients: usize,
}

fn status_of(state: &AppState, server: Option<&vtb_overlay::OverlayServer>) -> OverlayStatus {
    match server {
        Some(s) => OverlayStatus {
            running: true,
            port: Some(s.port),
            danmaku_url: Some(s.danmaku_url()),
            subtitle_url: Some(s.subtitle_url()),
            clients: state.overlay_publisher.receiver_count(),
        },
        None => OverlayStatus {
            running: false,
            port: None,
            danmaku_url: None,
            subtitle_url: None,
            clients: 0,
        },
    }
}

#[tauri::command]
pub async fn overlay_start(
    app: AppHandle,
    state: State<'_, AppState>,
    port: Option<u16>,
) -> Result<OverlayStatus, String> {
    let mut guard = state.overlay.lock().await;
    if guard.is_none() {
        // /api/status snapshot for external automation (录播姬-style API).
        let provider: vtb_overlay::StatusProvider = {
            let app = app.clone();
            std::sync::Arc::new(move || {
                let state = app.state::<AppState>();
                let danmaku: Vec<u64> = state
                    .danmaku
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|(_, h)| !h.pump.is_finished())
                    .map(|(id, _)| *id)
                    .collect();
                let recorders: Vec<u64> = state
                    .recorders
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|(_, h)| !h.monitor.is_finished())
                    .map(|(id, _)| *id)
                    .collect();
                let youtube: Vec<serde_json::Value> = state
                    .youtube
                    .connections
                    .lock()
                    .unwrap()
                    .values()
                    .cloned()
                    .collect();
                let platform: Vec<String> = state
                    .platform_recorders
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|(_, h)| !h.pump.is_finished())
                    .map(|(id, _)| id.clone())
                    .collect();
                serde_json::json!({
                    "app": "vtb-toolkit",
                    "danmaku_rooms": danmaku,
                    "recording_rooms": recorders,
                    "youtube": youtube,
                    "platform_recorders": platform,
                })
            })
        };
        let server = vtb_overlay::start_with_status(
            &state.overlay_publisher,
            port.filter(|p| *p != 0).unwrap_or(18990),
            Some(provider),
        )
        .await
        .map_err(|e| e.to_string())?;
        *guard = Some(server);
    }
    Ok(status_of(&state, guard.as_ref()))
}

#[tauri::command]
pub async fn overlay_stop(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(server) = state.overlay.lock().await.take() {
        server.shutdown().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn overlay_status(state: State<'_, AppState>) -> Result<OverlayStatus, String> {
    let guard = state.overlay.lock().await;
    Ok(status_of(&state, guard.as_ref()))
}

/// Serve the embedded guide locally so it opens in any browser while offline.
#[tauri::command]
pub async fn guide_open(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let settings = super::config::read_settings(&app);
    let port = settings
        .get("obs.port")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|p| *p >= 1024);
    let status = overlay_start(app, state, port).await?;
    Ok(format!(
        "http://127.0.0.1:{}/guide",
        status.port.unwrap_or(18990)
    ))
}
