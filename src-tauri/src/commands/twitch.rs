//! Twitch chat commands: anonymous IRC chat normalized into the same
//! LiveEvent pipeline (display, overlay, translation) as B站弹幕.

use crate::state::AppState;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn twitch_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    channel: String,
) -> Result<(), String> {
    let channel = channel.trim().trim_start_matches('#').to_lowercase();
    if channel.is_empty() {
        return Err("频道名为空".into());
    }
    {
        let map = state.twitch.lock().unwrap();
        if map
            .get(&channel)
            .map(|h| !h.pump.is_finished())
            .unwrap_or(false)
        {
            return Err(format!("channel {channel} already connected"));
        }
    }

    let (mut rx, stop) = vtb_danmaku::twitch::spawn_twitch_chat(&channel);
    let publisher = state.overlay_publisher.clone();
    let translate = state.danmaku_translate.clone();
    let ch = channel.clone();
    let pump = tokio::spawn(async move {
        while let Some(live) = rx.recv().await {
            // Auto-translation rides the same worker as B站弹幕.
            let mut tid = None;
            if let vtb_common::LiveEvent::Danmaku(d) = &live {
                let guard = translate.lock().unwrap();
                if let Some(h) = guard.as_ref() {
                    if !h.task.is_finished()
                        && vtb_translate::danmaku::should_translate_danmaku(&d.text, &h.target_lang)
                    {
                        let id = super::danmaku_translate::next_tid();
                        if h.tx.try_send((id, d.text.clone())).is_ok() {
                            tid = Some(id);
                        }
                    }
                }
            }
            publisher.publish(vtb_overlay::OverlayMessage::Danmaku {
                source: None,
                event: live.clone(),
                tid,
            });
            #[derive(serde::Serialize, Clone)]
            struct Emit<'a> {
                #[serde(flatten)]
                event: &'a vtb_common::LiveEvent,
                #[serde(skip_serializing_if = "Option::is_none")]
                tid: Option<u64>,
                channel: &'a str,
            }
            if app
                .emit(
                    super::danmaku::EVENT_DANMAKU,
                    Emit {
                        event: &live,
                        tid,
                        channel: &ch,
                    },
                )
                .is_err()
            {
                break;
            }
        }
    });

    state
        .twitch
        .lock()
        .unwrap()
        .insert(channel, crate::state::DanmakuHandles { pump, stop });
    Ok(())
}

#[tauri::command]
pub async fn twitch_disconnect(state: State<'_, AppState>, channel: String) -> Result<(), String> {
    let channel = channel.trim().trim_start_matches('#').to_lowercase();
    if let Some(h) = state.twitch.lock().unwrap().remove(&channel) {
        let _ = h.stop.send(true);
        h.pump.abort();
        Ok(())
    } else {
        Err(format!("channel {channel} not connected"))
    }
}

#[tauri::command]
pub async fn twitch_status(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let map = state.twitch.lock().unwrap();
    Ok(map
        .iter()
        .filter(|(_, h)| !h.pump.is_finished())
        .map(|(c, _)| c.clone())
        .collect())
}
