//! URL recording with session logs, markers, notifications and retention.
use crate::state::AppState;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State};
use vtb_recorder::{
    auto::{ResolvedStream, StreamResolver},
    platform::{platform_for, Platform},
    supervisor::{supervise_recording, SupervisorConfig, SupervisorEnd, SupervisorNote},
};
pub const EVENT_PLATFORM_REC: &str = "platform_rec://event";

struct PlatformResolver {
    platform: Box<dyn Platform>,
    id: String,
}
#[async_trait::async_trait]
impl StreamResolver for PlatformResolver {
    async fn resolve(&self, _: u64) -> vtb_recorder::error::Result<ResolvedStream> {
        self.platform.resolve(&self.id).await
    }
}

pub(crate) fn source_url(input: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(input.trim()).map_err(|_| "请输入完整的直播 HTTPS URL")?;
    if !["http", "https"].contains(&url.scheme())
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("无效的直播 URL".into());
    }
    if matches!(
        url.host_str(),
        Some("youtube.com" | "www.youtube.com" | "m.youtube.com" | "youtu.be")
    ) {
        let id = if url.host_str() == Some("youtu.be") {
            Some(url.path().trim_matches('/').to_string())
        } else {
            url.query_pairs()
                .find(|(k, _)| k == "v")
                .map(|(_, v)| v.into_owned())
                .or_else(|| {
                    url.path()
                        .strip_prefix("/live/")
                        .or_else(|| url.path().strip_prefix("/shorts/"))
                        .or_else(|| url.path().strip_prefix("/embed/"))
                        .map(|v| v.trim_matches('/').to_string())
                })
        };
        if let Some(id) = id {
            if id.len() != 11
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return Err("无效 YouTube 视频 ID".into());
            }
            return Ok(format!("https://www.youtube.com/watch?v={id}"));
        }
        let path = url
            .path()
            .trim_end_matches('/')
            .trim_end_matches("/live")
            .trim_end_matches("/streams");
        let valid = path
            .strip_prefix("/@")
            .is_some_and(|s| !s.is_empty() && !s.contains('/'))
            || ["/channel/UC", "/c/", "/user/"].iter().any(|prefix| {
                path.strip_prefix(prefix)
                    .is_some_and(|s| !s.is_empty() && !s.contains('/'))
            });
        if !valid {
            return Err("请输入 YouTube 直播或频道链接".into());
        }
        return Ok(format!(
            "https://www.youtube.com{}/live",
            url.path()
                .trim_end_matches('/')
                .trim_end_matches("/live")
                .trim_end_matches("/streams")
        ));
    }
    Ok(url.to_string())
}

fn folder_key(id: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hash);
    let slug: String = id
        .trim_start_matches("https://")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .take(48)
        .collect();
    format!("{slug}-{:016x}", hash.finish())
}
fn emit(app: &AppHandle, id: &str, kind: &str, message: impl Into<String>) {
    let _ = app.emit(
        EVENT_PLATFORM_REC,
        json!({"id":id,"kind":kind,"message":message.into()}),
    );
}
fn manifest(path: &Path, value: &Value) -> Result<(), String> {
    std::fs::write(
        path.join("recording.meta.json"),
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
fn hook(app: &AppHandle, settings: &Value, event: &str, room: &str, path: &str) {
    let _ = app;
    if let Some(script) = super::hooks::hook_for(settings, event) {
        super::hooks::run_hook(
            script,
            super::hooks::HookEvent {
                event: event.into(),
                room_id: 0,
                path: Some(path.into()),
                message: Some(room.into()),
            },
        );
    }
}

#[tauri::command]
pub async fn platform_record_start(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    output_dir: String,
    segment_mode: Option<String>,
    segment_value: Option<u64>,
) -> Result<(), String> {
    let id = source_url(&id)?;
    if output_dir.trim().is_empty() {
        return Err("请设置录制输出目录".into());
    }
    if !vtb_recorder::platform::YtDlpPlatform::default().available() {
        return Err("请安装 yt-dlp 并加入 PATH；Windows: winget install yt-dlp.yt-dlp".into());
    }
    let segment = match segment_mode.as_deref() {
        Some("duration") => vtb_recorder::ffmpeg::SegmentPolicy::ByDuration {
            seconds: segment_value.unwrap_or(3600).clamp(1, u32::MAX as u64) as u32,
        },
        Some("size") => vtb_recorder::ffmpeg::SegmentPolicy::BySize {
            bytes: segment_value
                .unwrap_or(2 * 1024 * 1024 * 1024)
                .max(1024 * 1024),
        },
        _ => vtb_recorder::ffmpeg::SegmentPolicy::Single,
    };
    let mut map = state.platform_recorders.lock().unwrap();
    if map.get(&id).map(|h| !h.pump.is_finished()).unwrap_or(false) {
        return Err("该来源已在监听录制".into());
    }
    let root = PathBuf::from(output_dir).join(format!("platform-{}", folder_key(&id)));
    let (stop_tx, mut stop) = tokio::sync::watch::channel(false);
    let key = id.clone();
    let pump = tokio::spawn(async move {
        let is_youtube = id.starts_with("https://www.youtube.com/");
        let notifier = std::sync::Arc::new(super::recorder::load_notifier(&app));
        let settings = super::config::read_settings(&app);
        let retention = super::recorder::load_retention(&settings);
        loop {
            if *stop.borrow() {
                break;
            }
            let check = async {
                if is_youtube {
                    super::youtube::request(&app, "info", json!({"input":id,"fresh":true})).await
                } else {
                    let platform = platform_for(&id, reqwest::Client::new(), 10000);
                    platform
                        .check_live(&id)
                        .await
                        .map(|s| json!({"live":s==vtb_common::LiveStatus::Live,"title":id}))
                        .map_err(|e| e.to_string())
                }
            };
            let info = tokio::select! { r=check => r, _=stop.changed()=>break };
            let info = match info {
                Ok(info) if info["live"] == true => info,
                other => {
                    match other {
                        Err(e) => emit(&app, &id, "error", e),
                        _ => emit(&app, &id, "waiting", "未开播，30 秒后重查"),
                    }
                    tokio::select! { _=tokio::time::sleep(Duration::from_secs(30))=>continue, _=stop.changed()=>break }
                }
            };
            let video_id = info["video_id"].as_str().map(str::to_string);
            let room_key = video_id
                .as_ref()
                .map(|v| format!("youtube:{v}"))
                .unwrap_or_else(|| id.clone());
            let state = app.state::<AppState>();
            let claimed = state
                .source_recording_claims
                .lock()
                .unwrap()
                .insert(room_key.clone());
            if !claimed {
                emit(
                    &app,
                    &id,
                    "waiting",
                    "此直播已由另一条链接录制，等待该会话结束",
                );
                tokio::select! { _=tokio::time::sleep(Duration::from_secs(30))=>continue, _=stop.changed()=>break }
            }
            struct Claim<'a>(&'a AppState, String);
            impl Drop for Claim<'_> {
                fn drop(&mut self) {
                    self.0
                        .source_recording_claims
                        .lock()
                        .unwrap()
                        .remove(&self.1);
                }
            }
            let _claim = Claim(&state, room_key.clone());
            let dir = root.join(chrono::Utc::now().format("%Y%m%d-%H%M%S-%3f").to_string());
            if let Err(e) = std::fs::create_dir_all(&dir) {
                emit(&app, &id, "error", e.to_string());
                break;
            }
            let session_id = dir.to_string_lossy().into_owned();
            let consumer = format!("record:{session_id}");
            let mut meta = json!({"room_id":0,"source":room_key,"source_url":id,"video_id":video_id,"title":info["title"],"started_at":chrono::Utc::now(),"ended_at":null,"segments":[],"health":null});
            if let Err(e) = manifest(&dir, &meta) {
                emit(&app, &id, "error", e);
                break;
            }
            if !retention.is_noop() {
                let (root, policy, keep) = (root.clone(), retention.clone(), dir.clone());
                let _ = tokio::task::spawn_blocking(move || {
                    vtb_recorder::retention::run_source_cleanup(&root, &policy, &keep)
                })
                .await;
            }
            if let Some(video) = &video_id {
                match vtb_pipeline::danmaku_log::DanmakuLogWriter::create(
                    &dir.join("danmaku.jsonl"),
                ) {
                    Ok(writer) => {
                        let db = super::recorder::stats_db_path(&app)
                            .and_then(|p| vtb_stats::StatsDb::open(&p).ok());
                        state.youtube.sinks.lock().unwrap().insert(
                            consumer.clone(),
                            super::youtube::YoutubeLog {
                                video_id: video.clone(),
                                session_id: session_id.clone(),
                                recording: false,
                                writer,
                                db,
                            },
                        );
                        let connect = super::youtube::request(
                            &app,
                            "connect",
                            json!({"input":video,"consumer":consumer}),
                        );
                        let result = tokio::select! { r=connect=>r, _=stop.changed()=>Err("录制已取消".into()) };
                        if let Err(e) = result {
                            emit(&app, &id, "warning", format!("评论日志未连接: {e}"));
                        }
                    }
                    Err(e) => emit(&app, &id, "warning", format!("无法创建评论日志: {e}")),
                }
            }
            let fixed_id = video_id
                .as_ref()
                .map(|v| format!("https://www.youtube.com/watch?v={v}"))
                .unwrap_or_else(|| id.clone());
            let resolver = PlatformResolver {
                platform: platform_for(&fixed_id, reqwest::Client::new(), 10000),
                id: fixed_id,
            };
            let mut cfg = SupervisorConfig::new(0, &dir, "recording");
            cfg.segment = segment.clone();
            cfg.extension_override = Some("mkv".into());
            cfg.startup_timeout = Duration::from_secs(90);
            cfg.stall_timeout = Duration::from_secs(30);
            let (notes_tx, mut notes) = tokio::sync::mpsc::channel(32);
            let recorder = vtb_recorder::ffmpeg::FfmpegRecorder::default();
            let recording =
                supervise_recording(&resolver, &recorder, &cfg, stop.clone(), Some(notes_tx));
            tokio::pin!(recording);
            let end = loop {
                tokio::select! {
                    end=&mut recording=>break end,
                    Some(note)=notes.recv()=>match note {
                        SupervisorNote::PartStarted {part}=>{
                            if part==0 {
                                if let Some(sink)=state.youtube.sinks.lock().unwrap().get_mut(&consumer) { sink.recording=true; }
                                meta["started_at"]=json!(chrono::Utc::now());
                                let _=manifest(&dir,&meta);
                                state.source_marker_dirs.lock().unwrap().insert(room_key.clone(),dir.clone());
                                emit(&app,&id,"started",session_id.clone());
                                super::recorder::push_notify(&notifier,vtb_notify::NotifyKind::RecordingStarted,format!("{room_key} 开始录制"),session_id.clone());
                                hook(&app,&settings,"recording_started",&room_key,&session_id);
                            } else { emit(&app,&id,"part",format!("续录分段 {}",part+1)); }
                        }
                        SupervisorNote::PartEnded {reason,..}=>emit(&app,&id,"part",reason),
                    }
                }
            };
            state.source_marker_dirs.lock().unwrap().remove(&room_key);
            state.youtube.sinks.lock().unwrap().remove(&consumer);
            if let Some(video) = &video_id {
                let _ = super::youtube::request(
                    &app,
                    "disconnect",
                    json!({"video_id":video,"consumer":consumer}),
                )
                .await;
            }
            let files = vtb_recorder::supervisor::session_outputs(&dir, "recording", "mkv");
            if files.is_empty() {
                emit(
                    &app,
                    &id,
                    "warning",
                    "本次录制没有生成视频，请检查直播状态和网络连接",
                );
            }
            meta["segments"]=json!(files.iter().map(|p|json!({"path":p,"size_bytes":std::fs::metadata(p).map(|m|m.len()).unwrap_or(0)})).collect::<Vec<_>>());
            meta["ended_at"] = json!(chrono::Utc::now());
            if let Err(e) = manifest(&dir, &meta) {
                emit(&app, &id, "error", e);
            }
            let mut health = Vec::new();
            for file in &files {
                let check = vtb_recorder::check::verify_recording(&recorder.binary, file).await;
                if !check.ok {
                    emit(&app, &id, "warning", check.issues.join("；"));
                }
                health.push(json!({"path":file,"result":check}));
                if let Err(e) = vtb_recorder::check::remux_mp4(&recorder.binary, file).await {
                    emit(
                        &app,
                        &id,
                        "warning",
                        format!("MP4 整理失败，原始 MKV 已保留: {e}"),
                    );
                }
            }
            meta["health"] = json!(health);
            let _ = manifest(&dir, &meta);
            if dir.join("danmaku.jsonl").exists() {
                if let Ok(entries) = vtb_pipeline::danmaku_log::read_log(&dir.join("danmaku.jsonl"))
                {
                    let start = chrono::DateTime::parse_from_rfc3339(
                        meta["started_at"].as_str().unwrap_or(""),
                    )
                    .ok()
                    .map(|t| t.with_timezone(&chrono::Utc));
                    if let Some(start) = start {
                        let xml = vtb_pipeline::danmaku_xml::to_xml(&entries, start);
                        let _ = std::fs::write(dir.join("danmaku.xml"), xml);
                    }
                }
            }
            hook(&app, &settings, "recording_stopped", &room_key, &session_id);
            super::recorder::push_notify(
                &notifier,
                vtb_notify::NotifyKind::RecordingStopped,
                format!("{room_key} 录制结束"),
                session_id.clone(),
            );
            emit(
                &app,
                &id,
                "stopped",
                format!("{} 段 → {session_id}", files.len()),
            );
            if let SupervisorEnd::GaveUp { last_error, .. } = &end {
                emit(&app, &id, "error", last_error.clone());
                super::recorder::push_notify(
                    &notifier,
                    vtb_notify::NotifyKind::RecordingError,
                    format!("{room_key} 录制异常"),
                    last_error.clone(),
                );
            }
            if matches!(end, SupervisorEnd::DiskFull { .. }) {
                emit(&app, &id, "error", "磁盘空间不足，已停止监听");
                break;
            }
            if *stop.borrow() {
                break;
            }
            tokio::select! { _=tokio::time::sleep(Duration::from_secs(30))=>{}, _=stop.changed()=>break }
        }
        emit(&app, &id, "idle", "监听已停止");
    });
    map.insert(
        key,
        crate::state::PlatformRecHandles {
            pump,
            stop: stop_tx,
        },
    );
    Ok(())
}

#[tauri::command]
pub async fn platform_record_stop(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let key = source_url(&id)?;
    let map = state.platform_recorders.lock().unwrap();
    let h = map.get(&key).ok_or("该来源未在监听")?;
    let _ = h.stop.send(true);
    // Keep the handle through finalization to prevent overlapping restarts.
    Ok(())
}
#[tauri::command]
pub fn platform_record_status(state: State<'_, AppState>) -> Vec<String> {
    state
        .platform_recorders
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, h)| !h.pump.is_finished())
        .map(|(id, _)| id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_video_urls_and_distinct_folders() {
        assert_eq!(
            source_url("https://youtu.be/abcdefghijk").unwrap(),
            source_url("https://www.youtube.com/live/abcdefghijk").unwrap()
        );
        assert!(source_url("--exec evil").is_err());
        assert_ne!(
            folder_key("https://example.com/a-b"),
            folder_key("https://example.com/a_b")
        );
    }
}
