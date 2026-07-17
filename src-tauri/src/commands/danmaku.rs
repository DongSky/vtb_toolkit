//! Danmaku commands: connect a room via the managed (auto-reconnecting)
//! client, stream LiveEvents + connection states to the frontend.

use crate::state::AppState;
use tauri::{AppHandle, Emitter, State};
use vtb_danmaku::api::BiliApi;
use vtb_danmaku::{spawn_managed, ConnState, DanmakuClientConfig, ManagedEvent};

/// Event channel name the frontend listens on.
pub const EVENT_DANMAKU: &str = "danmaku://event";
pub const EVENT_DANMAKU_STATUS: &str = "danmaku://status";

#[derive(serde::Serialize, Clone)]
struct StatusPayload {
    room_id: u64,
    connected: bool,
    message: String,
}

fn state_payload(room_id: u64, s: &ConnState) -> StatusPayload {
    match s {
        ConnState::Connecting { attempt } => StatusPayload {
            room_id,
            connected: false,
            message: format!("连接中 (第{}次)", attempt + 1),
        },
        ConnState::Connected { host } => StatusPayload {
            room_id,
            connected: true,
            message: format!("已连接 {host}"),
        },
        ConnState::Disconnected { reason } => StatusPayload {
            room_id,
            connected: false,
            message: format!("连接断开: {reason}"),
        },
        ConnState::Reconnecting { attempt, delay_ms } => StatusPayload {
            room_id,
            connected: false,
            message: format!("{}s 后重连 (第{}次)", delay_ms / 1000, attempt + 1),
        },
        ConnState::Stopped => StatusPayload {
            room_id,
            connected: false,
            message: "已停止".into(),
        },
    }
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

    // Use login credentials when available: real uid + cookies mean
    // unmasked usernames in danmaku.
    let mut config = DanmakuClientConfig::anonymous(room_id);
    let api = match state.creds() {
        Some(c) => {
            config.uid = c.dede_user_id;
            config.buvid = c.buvid3.clone();
            BiliApi::new(state.api_client()?)
        }
        None => BiliApi::default_client().map_err(|e| e.to_string())?,
    };

    let (mut rx, stop) = spawn_managed(api, config);

    let app2 = app.clone();
    let publisher = state.overlay_publisher.clone();
    let pump = tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            let keep_going = match ev {
                ManagedEvent::Live(live) => {
                    // Mirror to the OBS overlay (no-op when not running).
                    publisher.publish(vtb_overlay::OverlayMessage::Danmaku(live.clone()));
                    app2.emit(EVENT_DANMAKU, &live).is_ok()
                }
                ManagedEvent::State(s) => {
                    let done = matches!(s, ConnState::Stopped);
                    let _ = app2.emit(EVENT_DANMAKU_STATUS, state_payload(room_id, &s));
                    !done
                }
            };
            if !keep_going {
                break;
            }
        }
    });

    state
        .danmaku
        .lock()
        .unwrap()
        .insert(room_id, crate::state::DanmakuHandles { pump, stop });
    Ok(())
}

#[tauri::command]
pub async fn danmaku_disconnect(
    state: State<'_, AppState>,
    room_id: u64,
) -> Result<(), String> {
    if let Some(handles) = state.danmaku.lock().unwrap().remove(&room_id) {
        // Graceful: the supervisor exits its loop and emits Stopped, which
        // ends the pump. Abort as a backstop.
        let _ = handles.stop.send(true);
        handles.pump.abort();
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
        .filter(|(_, h)| !h.pump.is_finished())
        .map(|(id, _)| *id)
        .collect())
}
