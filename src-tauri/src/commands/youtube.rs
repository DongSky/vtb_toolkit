//! Bridge to the locally bundled YouTube.js browser runtime. HTTP remains native;
//! no Node process, public proxy, Google API key, or login is needed for reading.
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State};
use vtb_common::{EventSource, LiveEvent, LivePlatform};

type Reply = tokio::sync::oneshot::Sender<Result<Value, String>>;
#[derive(Default)]
pub struct YoutubeState {
    pub ready: AtomicBool,
    next: AtomicU64,
    pending: Mutex<HashMap<u64, Reply>>,
    pub connections: Mutex<HashMap<String, Value>>,
    detectors: Mutex<
        HashMap<
            String,
            (
                std::time::Instant,
                vtb_highlight::realtime::RealtimeDetector,
            ),
        >,
    >,
    pub sinks: Mutex<HashMap<String, YoutubeLog>>,
}

pub struct YoutubeLog {
    pub video_id: String,
    pub session_id: String,
    pub recording: bool,
    pub writer: vtb_pipeline::danmaku_log::DanmakuLogWriter<std::io::BufWriter<std::fs::File>>,
    pub db: Option<vtb_stats::StatsDb>,
}

pub async fn request(app: &AppHandle, method: &str, args: Value) -> Result<Value, String> {
    let state = app.state::<AppState>();
    for _ in 0..100 {
        if state.youtube.ready.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if !state.youtube.ready.load(Ordering::Relaxed) {
        return Err("YouTube 服务尚未就绪，请稍后重试".into());
    }
    let id = state.youtube.next.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = tokio::sync::oneshot::channel();
    state.youtube.pending.lock().unwrap().insert(id, tx);
    // Remove pending replies even if the caller cancels its future.
    struct Pending<'a>(&'a YoutubeState, u64, &'a AppHandle);
    impl Drop for Pending<'_> {
        fn drop(&mut self) {
            if self.0.pending.lock().unwrap().remove(&self.1).is_some() {
                let _ = self.2.emit_to("main", "youtube://cancel", self.1);
            }
        }
    }
    let _pending = Pending(&state.youtube, id, app);
    app.emit_to(
        "main",
        "youtube://request",
        json!({"id": id, "method": method, "args": args}),
    )
    .map_err(|e| e.to_string())?;
    tokio::time::timeout(Duration::from_secs(60), rx)
        .await
        .map_err(|_| "YouTube 请求超时".to_string())?
        .map_err(|_| "YouTube 服务已关闭".to_string())?
}

#[tauri::command]
pub async fn youtube_call(app: AppHandle, method: String, args: Value) -> Result<Value, String> {
    if !["info", "connect", "disconnect"].contains(&method.as_str()) {
        return Err("不支持的 YouTube 操作".into());
    }
    request(&app, &method, args).await
}

#[tauri::command]
pub fn youtube_bridge_ready(state: State<'_, AppState>) -> Vec<Value> {
    for (_, reply) in state.youtube.pending.lock().unwrap().drain() {
        let _ = reply.send(Err("评论服务正在重新加载，请重试".into()));
    }
    let previous = state
        .youtube
        .connections
        .lock()
        .unwrap()
        .drain()
        .map(|(_, value)| value)
        .collect();
    state.youtube.ready.store(true, Ordering::Relaxed);
    previous
}

#[tauri::command]
pub fn youtube_reply(
    state: State<'_, AppState>,
    id: u64,
    value: Option<Value>,
    error: Option<String>,
) -> bool {
    if let Some(tx) = state.youtube.pending.lock().unwrap().remove(&id) {
        tx.send(match error {
            Some(e) => Err(e),
            None => Ok(value.unwrap_or(Value::Null)),
        })
        .is_ok()
    } else {
        false
    }
}

#[derive(Deserialize)]
pub struct HttpRequest {
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}
#[derive(Serialize)]
pub struct HttpResponse {
    status: u16,
    headers: HashMap<String, String>,
    body: String,
}

fn allowed_request(input: &HttpRequest) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(&input.url).map_err(|_| "无效 URL")?;
    let paths = [
        "/sw.js_data",
        "/youtubei/v1/config",
        "/youtubei/v1/player",
        "/youtubei/v1/next",
        "/youtubei/v1/browse",
        "/youtubei/v1/navigation/resolve_url",
        "/youtubei/v1/live_chat/get_live_chat",
        "/youtubei/v1/updated_metadata",
    ];
    if url.scheme() != "https"
        || url.host_str() != Some("www.youtube.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || !paths.contains(&url.path())
        || !["GET", "POST"].contains(&input.method.as_str())
    {
        return Err("仅允许 YouTube 匿名读取接口".into());
    }
    if input
        .body
        .as_ref()
        .map(|s| s.len() > 512 * 1024)
        .unwrap_or(false)
    {
        return Err("请求体过大".into());
    }
    Ok(url)
}

#[tauri::command]
pub async fn youtube_http(request: HttpRequest) -> Result<HttpResponse, String> {
    let url = allowed_request(&request)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let method =
        reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|e| e.to_string())?;
    let mut req = client.request(method, url);
    for (name, value) in request.headers {
        if [
            "accept",
            "accept-language",
            "content-type",
            "user-agent",
            "origin",
            "referer",
            "x-youtube-client-name",
            "x-youtube-client-version",
            "x-goog-visitor-id",
            "x-origin",
        ]
        .contains(&name.to_ascii_lowercase().as_str())
        {
            req = req.header(name, value);
        }
    }
    let mut response = req
        .body(request.body.unwrap_or_default())
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
        .collect();
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| e.without_url().to_string())?
    {
        if bytes.len() + chunk.len() > 16 * 1024 * 1024 {
            return Err("YouTube 响应过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(HttpResponse {
        status,
        headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    })
}

#[tauri::command]
pub fn youtube_status(state: State<'_, AppState>) -> Vec<Value> {
    state
        .youtube
        .connections
        .lock()
        .unwrap()
        .values()
        .cloned()
        .collect()
}

#[tauri::command]
pub fn youtube_connection(
    app: AppHandle,
    state: State<'_, AppState>,
    video_id: String,
    status: Value,
) {
    let mut connections = state.youtube.connections.lock().unwrap();
    if status["state"] == "stopped" {
        connections.remove(&video_id);
        state
            .youtube
            .detectors
            .lock()
            .unwrap()
            .remove(&format!("youtube:{video_id}"));
    } else {
        connections.insert(video_id.clone(), status.clone());
    }
    let _ = app.emit(
        "youtube://status",
        json!({"video_id":video_id,"status":status}),
    );
}

#[tauri::command]
pub fn youtube_event(
    app: AppHandle,
    state: State<'_, AppState>,
    event: LiveEvent,
    source: EventSource,
) -> Result<(), String> {
    if source.platform != LivePlatform::Youtube || source.room_id.len() != 11 {
        return Err("无效 YouTube 来源".into());
    }
    if !state
        .youtube
        .connections
        .lock()
        .unwrap()
        .contains_key(&source.room_id)
    {
        return Ok(());
    }
    let key = source.room_key();
    let now = chrono::Utc::now();
    let mut sinks = state.youtube.sinks.lock().unwrap();
    for sink in sinks
        .values_mut()
        .filter(|s| s.recording && s.video_id == source.room_id)
    {
        if let Some(db) = &sink.db {
            db.record_with_source(&sink.session_id, &event, now, Some(&source))
                .map_err(|e| e.to_string())?;
        }
        sink.writer
            .write(&vtb_pipeline::danmaku_log::LogEntry {
                received_at: now,
                event: event.clone(),
                source: Some(source.clone()),
            })
            .map_err(|e| e.to_string())?;
        sink.writer.flush().map_err(|e| e.to_string())?;
    }
    drop(sinks);
    // Same translation, highlight, marker and TTS services as the Bilibili pump.
    let text = match &event {
        LiveEvent::Danmaku(d) => Some(&d.text),
        LiveEvent::SuperChat(s) => Some(&s.text),
        _ => None,
    };
    let tid = text.and_then(|text| {
        let handle = state.danmaku_translate.lock().unwrap();
        let h = handle.as_ref()?;
        if h.task.is_finished()
            || !vtb_translate::danmaku::should_translate_danmaku(text, &h.target_lang)
        {
            return None;
        }
        let tid = super::danmaku_translate::next_tid();
        h.tx.try_send((tid, text.clone())).ok().map(|_| tid)
    });
    if let LiveEvent::Danmaku(d) = &event {
        if d.is_admin {
            if let Some(note) = super::markers::danmaku_marker_note(&d.text) {
                let _ = super::markers::add_source_marker(
                    &app,
                    &state,
                    &key,
                    vtb_pipeline::markers::Marker::manual("danmaku", note),
                );
            }
        }
    }
    let alert = {
        let mut detectors = state.youtube.detectors.lock().unwrap();
        let (started, detector) = detectors.entry(key.clone()).or_insert_with(|| {
            (
                std::time::Instant::now(),
                vtb_highlight::realtime::RealtimeDetector::new(Default::default()),
            )
        });
        detector.on_event(started.elapsed().as_millis() as u64, &event)
    };
    if let Some(alert) = alert {
        let _ = super::markers::add_source_marker(
            &app,
            &state,
            &key,
            vtb_pipeline::markers::Marker::auto(alert.reason.clone()),
        );
        let _ = app.emit(
            "highlight://alert",
            json!({"room_id":0,"room_key":key,"reason":alert.reason}),
        );
        super::recorder::push_notify(
            &std::sync::Arc::new(super::recorder::load_notifier(&app)),
            vtb_notify::NotifyKind::Highlight,
            format!("YouTube {} 疑似高能", source.room_id),
            alert.reason,
        );
    }
    if state.tts_enabled.load(Ordering::Relaxed) {
        if let Some(line) = match &event {
            LiveEvent::SuperChat(s) => Some(format!(
                "{}的{}留言：{}",
                s.username,
                source
                    .money
                    .as_ref()
                    .map(|m| m.display.as_str())
                    .unwrap_or("付费"),
                s.text
            )),
            LiveEvent::GuardBuy(g) => Some(format!(
                "{}：{}",
                g.username,
                source.membership.as_deref().unwrap_or("频道会员")
            )),
            LiveEvent::Gift(g) => Some(format!("{}：{}", g.username, g.gift_name)),
            _ => super::tts::should_speak(&event, state.tts_paid_only.load(Ordering::Relaxed)),
        } {
            let _ = state
                .tts_tx
                .get_or_init(super::tts::spawn_tts_worker)
                .try_send(line);
        }
    }
    state
        .overlay_publisher
        .publish(vtb_overlay::OverlayMessage::Danmaku {
            event: event.clone(),
            tid,
            source: Some(Box::new(source.clone())),
        });
    let mut payload = serde_json::to_value(event).map_err(|e| e.to_string())?;
    payload["source"] = serde_json::to_value(source).map_err(|e| e.to_string())?;
    payload["tid"] = json!(tid);
    app.emit(super::danmaku::EVENT_DANMAKU, payload)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn youtube_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    video_id: String,
    message_id: Option<String>,
    user_id: Option<String>,
) {
    let room_key = format!("youtube:{video_id}");
    let deletion = vtb_pipeline::danmaku_log::ChatDeletion {
        received_at: chrono::Utc::now(),
        room_key: room_key.clone(),
        message_id: message_id.clone(),
        user_id: user_id.clone(),
    };
    for sink in state
        .youtube
        .sinks
        .lock()
        .unwrap()
        .values_mut()
        .filter(|s| s.recording && s.video_id == video_id)
    {
        if let Err(e) = deletion.append(std::path::Path::new(&sink.session_id)) {
            tracing::warn!("moderation log: {e}");
        }
        if let Some(db) = &sink.db {
            if let Err(e) = db.delete_source_events(
                &sink.session_id,
                &room_key,
                message_id.as_deref(),
                user_id.as_deref(),
            ) {
                tracing::warn!("moderation stats: {e}");
            }
        }
    }
    state
        .overlay_publisher
        .publish(vtb_overlay::OverlayMessage::ChatDelete {
            room_key: room_key.clone(),
            message_id: message_id.clone(),
            user_id: user_id.clone(),
        });
    let _ = app.emit(
        "danmaku://delete",
        json!({"room_key":room_key,"message_id":message_id,"user_id":user_id}),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_proxy_restricts_origin_paths_and_methods() {
        let mut r = HttpRequest {
            url: "https://www.youtube.com/youtubei/v1/next".into(),
            method: "POST".into(),
            headers: HashMap::new(),
            body: None,
        };
        assert!(allowed_request(&r).is_ok());
        for url in [
            "https://www.youtube.com.evil.test/youtubei/v1/next",
            "http://127.0.0.1/",
            "https://www.youtube.com/youtubei/v1/live_chat/send_message",
            "https://user@www.youtube.com/sw.js_data",
        ] {
            r.url = url.into();
            assert!(allowed_request(&r).is_err());
        }
    }
}
