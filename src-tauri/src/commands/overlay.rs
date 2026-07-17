//! OBS overlay server commands.

use crate::state::AppState;
use tauri::State;

#[derive(serde::Serialize)]
pub struct OverlayStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub danmaku_url: Option<String>,
    pub subtitle_url: Option<String>,
    /// Connected overlay pages (WS clients).
    pub clients: usize,
}

fn status_of(
    state: &AppState,
    server: Option<&vtb_overlay::OverlayServer>,
) -> OverlayStatus {
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
    state: State<'_, AppState>,
    port: Option<u16>,
) -> Result<OverlayStatus, String> {
    let mut guard = state.overlay.lock().await;
    if guard.is_none() {
        let server = vtb_overlay::start(&state.overlay_publisher, port.unwrap_or(0))
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
