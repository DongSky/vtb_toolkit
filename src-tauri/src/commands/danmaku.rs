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

/// LiveEvent + optional translation correlation id for the frontend list.
#[derive(serde::Serialize, Clone)]
struct DanmakuEmit<'a> {
    #[serde(flatten)]
    event: &'a vtb_common::LiveEvent,
    #[serde(skip_serializing_if = "Option::is_none")]
    tid: Option<u64>,
}

/// Translatable text of an event (plain danmaku & SuperChat; stickers
/// excluded — their text is just the emote name).
fn translatable_text(ev: &vtb_common::LiveEvent) -> Option<&str> {
    match ev {
        vtb_common::LiveEvent::Danmaku(d) if d.emoticon.is_none() => Some(&d.text),
        vtb_common::LiveEvent::SuperChat(s) => Some(&s.text),
        _ => None,
    }
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
    let translate = state.danmaku_translate.clone();
    // 场控: auto-thank (config shared; worker spawned lazily).
    let autothank = state.autothank.clone();
    let send_tx = state
        .danmaku_send_tx
        .get_or_init(|| super::danmaku_send::spawn_send_worker(app.clone()))
        .clone();
    let notifier = std::sync::Arc::new(super::recorder::load_notifier(&app));
    // TTS: serial speech worker (lazy) + switch handles for the pump.
    let tts_tx = state
        .tts_tx
        .get_or_init(super::tts::spawn_tts_worker)
        .clone();
    let tts_enabled = state.tts_enabled.clone();
    let tts_paid_only = state.tts_paid_only.clone();
    let marker_dirs = state.marker_dirs.clone();
    let pump = tokio::spawn(async move {
        // Realtime highlight detection over the live event stream.
        let mut detector = vtb_highlight::realtime::RealtimeDetector::new(
            vtb_highlight::realtime::RealtimeConfig::default(),
        );
        let started = tokio::time::Instant::now();
        while let Some(ev) = rx.recv().await {
            let keep_going = match ev {
                ManagedEvent::Live(live) => {
                    let offset = started.elapsed().as_millis() as u64;
                    if let Some(alert) = detector.on_event(offset, &live) {
                        let reason = format!(
                            "{}（弹幕{}条 z={:.1}）",
                            alert.reason, alert.danmaku_count, alert.z
                        );
                        let _ = app2.emit(
                            "highlight://alert",
                            serde_json::json!({
                                "room_id": room_id,
                                "reason": reason,
                                "z": alert.z,
                                "danmaku_count": alert.danmaku_count,
                            }),
                        );
                        // 打点: persist as an auto marker when this room is
                        // being recorded. Wall clock, not the pump-local
                        // offset — the pump may have started long after the
                        // recording, so only created_at aligns with it.
                        if let Err(e) = super::markers::add_marker(
                            &app2,
                            &marker_dirs,
                            room_id,
                            vtb_pipeline::markers::Marker::auto(reason.clone()),
                        ) {
                            tracing::debug!("auto marker skipped: {e}");
                        }
                        super::recorder::push_notify(
                            &notifier,
                            vtb_notify::NotifyKind::Highlight,
                            format!("房间 {room_id} 疑似高能时刻"),
                            reason,
                        );
                    }
                    // 打点弹幕口令: 房管/主播发 "打点 [备注]" 记一个手动
                    // 标记（观众刷屏不触发）。
                    if let vtb_common::LiveEvent::Danmaku(d) = &live {
                        if d.is_admin {
                            if let Some(note) = super::markers::danmaku_marker_note(&d.text) {
                                let marker = vtb_pipeline::markers::Marker::manual(
                                    "danmaku",
                                    note.or_else(|| Some(format!("{} 打点", d.username))),
                                );
                                if let Err(e) = super::markers::add_marker(
                                    &app2,
                                    &marker_dirs,
                                    room_id,
                                    marker,
                                ) {
                                    tracing::debug!("danmaku marker skipped: {e}");
                                }
                            }
                        }
                    }
                    // TTS (serial queue; try_send drops when busy).
                    if tts_enabled.load(std::sync::atomic::Ordering::Relaxed) {
                        let paid = tts_paid_only.load(std::sync::atomic::Ordering::Relaxed);
                        if let Some(text) = super::tts::should_speak(&live, paid) {
                            let _ = tts_tx.try_send(text);
                        }
                    }
                    // 场控: auto-thank gifts / guards / SC (rate-limited
                    // worker; drops when saturated).
                    let thanks = {
                        let cfg = autothank.lock().unwrap();
                        super::danmaku_send::format_thanks(&cfg, &live)
                    };
                    if let Some(line) = thanks {
                        if send_tx.try_send((room_id, line)).is_err() {
                            tracing::debug!("auto-thank dropped (queue full)");
                        }
                    }
                    // Auto-translation: hand the line to the worker (if
                    // running) and tag the outgoing event with a tid so the
                    // UI/overlay can attach the result later.
                    let mut tid = None;
                    if let Some(text) = translatable_text(&live) {
                        let guard = translate.lock().unwrap();
                        if let Some(h) = guard.as_ref() {
                            if !h.task.is_finished()
                                && vtb_translate::danmaku::should_translate_danmaku(
                                    text,
                                    &h.target_lang,
                                )
                            {
                                let id = super::danmaku_translate::next_tid();
                                // Drop when the queue is full: stale chat
                                // translations are worthless.
                                if h.tx.try_send((id, text.to_string())).is_ok() {
                                    tid = Some(id);
                                }
                            }
                        }
                    }
                    // Mirror to the OBS overlay (no-op when not running).
                    publisher.publish(vtb_overlay::OverlayMessage::Danmaku {
                        event: live.clone(),
                        tid,
                    });
                    app2.emit(EVENT_DANMAKU, DanmakuEmit { event: &live, tid }).is_ok()
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
