//! Danmaku commands: connect a room, stream LiveEvents to the frontend.

use crate::state::AppState;
use tauri::{AppHandle, Emitter, State};
use vtb_danmaku::{DanmakuClient, DanmakuClientConfig};

/// Event channel name the frontend listens on.
pub const EVENT_DANMAKU: &str = "danmaku://event";
pub const EVENT_DANMAKU_STATUS: &str = "danmaku://status";

#[derive(serde::Serialize, Clone)]
struct StatusPayload {
    room_id: u64,
    connected: bool,
    message: String,
}

#[tauri::command]
pub async fn danmaku_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    room_id: u64,
) -> Result<(), String> {
    if state.danmaku_connected(room_id) {
        return Err(format!("room {room_id} already connected"));
    }

    let client = DanmakuClient::new(DanmakuClientConfig::anonymous(room_id))
        .map_err(|e| e.to_string())?;
    let (mut rx, conn_handle) = client.connect().await.map_err(|e| e.to_string())?;

    let _ = app.emit(
        EVENT_DANMAKU_STATUS,
        StatusPayload {
            room_id,
            connected: true,
            message: "connected".into(),
        },
    );

    let app2 = app.clone();
    let pump = tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            if app2.emit(EVENT_DANMAKU, &ev).is_err() {
                break;
            }
        }
        conn_handle.abort();
        let _ = app2.emit(
            EVENT_DANMAKU_STATUS,
            StatusPayload {
                room_id,
                connected: false,
                message: "disconnected".into(),
            },
        );
    });

    state.danmaku.lock().unwrap().insert(room_id, pump);
    Ok(())
}

#[tauri::command]
pub async fn danmaku_disconnect(
    state: State<'_, AppState>,
    room_id: u64,
) -> Result<(), String> {
    if let Some(handle) = state.danmaku.lock().unwrap().remove(&room_id) {
        handle.abort();
        Ok(())
    } else {
        Err(format!("room {room_id} not connected"))
    }
}

#[tauri::command]
pub async fn danmaku_status(
    state: State<'_, AppState>,
) -> Result<Vec<u64>, String> {
    let map = state.danmaku.lock().unwrap();
    Ok(map
        .iter()
        .filter(|(_, h)| !h.is_finished())
        .map(|(id, _)| *id)
        .collect())
}
