//! Recorder commands: start/stop auto-recording for a room.

use crate::state::{AppState, RecorderHandles};
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;
use vtb_recorder::auto::{AutoRecorder, AutoRecorderConfig, BiliResolver, RecorderEvent};
use vtb_recorder::ffmpeg::{FfmpegRecorder, SegmentPolicy};
use vtb_recorder::monitor::{Monitor, MonitorConfig, StatusSource};
use vtb_recorder::stream::{qn, StreamApi};

pub const EVENT_RECORDER: &str = "recorder://event";

#[derive(serde::Serialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RecorderPayload {
    Started {
        room_id: u64,
        output_dir: String,
    },
    Stopped {
        room_id: u64,
        total_bytes: u64,
        segments: usize,
    },
    Error {
        room_id: u64,
        message: String,
    },
}

#[derive(serde::Deserialize)]
pub struct RecorderOptions {
    pub room_id: u64,
    pub output_dir: String,
    /// "single" | "duration" | "size"
    #[serde(default)]
    pub segment_mode: Option<String>,
    /// seconds for duration mode; bytes for size mode.
    #[serde(default)]
    pub segment_value: Option<u64>,
    /// poll seconds (default 15)
    #[serde(default)]
    pub poll_secs: Option<u64>,
    /// Realtime subtitles from the SAME pull (no second connection).
    #[serde(default)]
    pub live_subtitle: bool,
    /// whisper model path (required when live_subtitle).
    #[serde(default)]
    pub model_path: Option<String>,
}

struct ApiStatusSource {
    api: StreamApi,
}

#[async_trait::async_trait]
impl StatusSource for ApiStatusSource {
    async fn current_status(
        &self,
        room_id: u64,
    ) -> vtb_recorder::error::Result<vtb_common::LiveStatus> {
        Ok(self.api.room_status(room_id).await?.status)
    }
}

/// Build the notifier fan-out from the "notify" object in settings.json.
pub(crate) fn load_notifier(app: &AppHandle) -> vtb_notify::MultiNotifier {
    let cfg = super::config::read_settings(app);
    match serde_json::from_value::<vtb_notify::NotifyConfig>(cfg["notify"].clone()) {
        Ok(c) => c.build(),
        Err(_) => vtb_notify::MultiNotifier(vec![]),
    }
}

/// Build the rolling-cleanup policy from the "retention" object in
/// settings.json. Absent/empty → no-op (nothing is ever deleted).
///
/// ```json
/// "retention": { "max_age_days": 30, "max_sessions_per_room": 10,
///                 "target_free_gib": 20 }
/// ```
pub(crate) fn load_retention(
    settings: &serde_json::Value,
) -> vtb_recorder::retention::RetentionPolicy {
    use std::time::Duration;
    let r = &settings["retention"];
    let max_age = r["max_age_days"]
        .as_u64()
        .filter(|d| *d > 0)
        .map(|d| Duration::from_secs(d * 86400));
    let max_sessions_per_room = r["max_sessions_per_room"]
        .as_u64()
        .filter(|n| *n > 0)
        .map(|n| n as usize);
    let target_free_bytes = r["target_free_gib"]
        .as_f64()
        .filter(|g| *g > 0.0)
        .map(|g| (g * 1024.0 * 1024.0 * 1024.0) as u64);
    vtb_recorder::retention::RetentionPolicy {
        max_age,
        max_sessions_per_room,
        target_free_bytes,
    }
}

/// Fire-and-forget notification (never blocks the event pump).
pub(crate) fn push_notify(
    notifier: &std::sync::Arc<vtb_notify::MultiNotifier>,
    kind: vtb_notify::NotifyKind,
    title: String,
    body: String,
) {
    if notifier.0.is_empty() {
        return;
    }
    let notifier = notifier.clone();
    tokio::spawn(async move {
        let n = notifier
            .notify_all(&vtb_notify::NotifyEvent { title, body, kind })
            .await;
        tracing::debug!("notified {n} channels");
    });
}

/// Handle to a per-session danmaku JSONL logger.
struct DanmakuLogHandle {
    stop: tokio::sync::watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

impl DanmakuLogHandle {
    async fn stop(self) {
        let _ = self.stop.send(true);
        let _ = tokio::time::timeout(Duration::from_secs(5), self.task).await;
    }
}

/// Path of the shared stats database.
pub fn stats_db_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    use tauri::Manager;
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("stats.sqlite3"))
}

/// Record every live event of `room_id` into `<dir>/danmaku.jsonl` until
/// stopped (managed connection: reconnects on its own). Events are also
/// written to the stats DB for the 记录簿/场次报告.
fn start_danmaku_log(
    room_id: u64,
    dir: &std::path::Path,
    creds: Option<vtb_account::Credentials>,
    stats_db: Option<std::path::PathBuf>,
) -> Option<DanmakuLogHandle> {
    use vtb_pipeline::danmaku_log::{DanmakuLogWriter, LogEntry};

    let path = dir.join("danmaku.jsonl");
    let mut writer = match DanmakuLogWriter::create(&path) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("danmaku log create failed: {e}");
            return None;
        }
    };

    let mut config = vtb_danmaku::DanmakuClientConfig::anonymous(room_id);
    let api = match creds {
        Some(c) => {
            config.uid = c.dede_user_id;
            config.buvid = c.buvid3.clone();
            match vtb_account::build_client(Some(&c)) {
                Ok(client) => vtb_danmaku::api::BiliApi::new(client),
                Err(_) => vtb_danmaku::api::BiliApi::default_client().ok()?,
            }
        }
        None => vtb_danmaku::api::BiliApi::default_client().ok()?,
    };

    let (mut rx, stop) = vtb_danmaku::spawn_managed(api, config);
    let session_id = dir.to_string_lossy().into_owned();
    let stats = stats_db.and_then(|p| vtb_stats::StatsDb::open(&p).ok());
    let task = tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            match ev {
                vtb_danmaku::ManagedEvent::Live(live) => {
                    let now = chrono::Utc::now();
                    if let Some(db) = &stats {
                        if let Err(e) = db.record(&session_id, &live, now) {
                            tracing::warn!("stats record failed: {e}");
                        }
                    }
                    let entry = LogEntry {
                        source: None,
                        received_at: now,
                        event: live,
                    };
                    if writer.write(&entry).is_ok() {
                        let _ = writer.flush();
                    }
                }
                vtb_danmaku::ManagedEvent::State(vtb_danmaku::ConnState::Stopped) => break,
                vtb_danmaku::ManagedEvent::State(_) => {}
            }
        }
    });
    Some(DanmakuLogHandle { stop, task })
}

fn make_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
        )
        .build()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn recorder_start(
    app: AppHandle,
    state: State<'_, AppState>,
    options: RecorderOptions,
) -> Result<(), String> {
    let room_id = options.room_id;
    if state.recorder_active(room_id) {
        return Err(format!("room {room_id} already monitored"));
    }

    // Cookie-authenticated clients unlock original-quality (qn=10000)
    // streams; anonymous callers still work at lower quality.
    let stream_client = state.api_client().or_else(|_| make_client())?;
    let status_client = stream_client.clone();

    let segment = match options.segment_mode.as_deref() {
        Some("duration") => SegmentPolicy::ByDuration {
            seconds: options.segment_value.unwrap_or(3600) as u32,
        },
        Some("size") => SegmentPolicy::BySize {
            bytes: options.segment_value.unwrap_or(2 * 1024 * 1024 * 1024),
        },
        _ => SegmentPolicy::Single,
    };

    let mut mon_cfg = MonitorConfig::new(room_id);
    mon_cfg.poll_interval = Duration::from_secs(options.poll_secs.unwrap_or(15));

    let monitor = Monitor::new(
        mon_cfg,
        ApiStatusSource {
            api: StreamApi::new(status_client),
        },
    );
    // Single-pull realtime subtitles: recorder tees decoded PCM to an ASR
    // task; no second stream connection.
    let pcm_tx = if options.live_subtitle {
        #[cfg(not(feature = "whisper"))]
        {
            return Err("built without whisper support".into());
        }
        #[cfg(feature = "whisper")]
        {
            let model = options
                .model_path
                .clone()
                .filter(|m| !m.is_empty())
                .ok_or("实时字幕需要 whisper 模型路径")?;
            let engine = std::sync::Arc::new(
                vtb_asr::engine::WhisperEngine::new(std::path::Path::new(&model), None)
                    .map_err(|e| e.to_string())?,
            );
            let (tx, mut rx) = mpsc::channel::<Vec<f32>>(64);
            let app_sub = app.clone();
            let publisher = state.overlay_publisher.clone();
            tokio::spawn(async move {
                let mut asr = vtb_asr::streaming::StreamingAsr::new(
                    engine,
                    vtb_asr::streaming::StreamingConfig::default(),
                );
                while let Some(chunk) = rx.recv().await {
                    for seg in asr.feed(&chunk).await {
                        publisher.publish(vtb_overlay::OverlayMessage::Subtitle {
                            room_key: None,
                            text: seg.text.clone(),
                            translated: None,
                            lang: seg.lang.clone(),
                        });
                        let _ = app_sub.emit(
                            super::subtitle::EVENT_SUBTITLE,
                            serde_json::json!({
                                "room_id": room_id,
                                "start_ms": seg.start_ms,
                                "end_ms": seg.end_ms,
                                "text": seg.text,
                                "lang": seg.lang,
                                "translated": null,
                            }),
                        );
                    }
                }
            });
            Some(tx)
        }
    } else {
        None
    };

    let auto = AutoRecorder::new(
        {
            let mut cfg = AutoRecorderConfig::new(room_id, PathBuf::from(&options.output_dir));
            cfg.segment = segment;
            cfg.pcm_tx = pcm_tx;
            cfg
        },
        BiliResolver::new(StreamApi::new(stream_client), qn::ORIGINAL),
        FfmpegRecorder::default(),
    );

    let (mon_tx, mon_rx) = mpsc::channel(16);
    let (rec_tx, mut rec_rx) = mpsc::channel(16);

    let monitor_task = tokio::spawn(async move {
        let _ = monitor.run(mon_tx, None).await;
    });
    let recorder_task = tokio::spawn(auto.run(mon_rx, rec_tx));

    // Pump recorder events to the frontend, keep a danmaku JSONL logger
    // running per session, and push notifications (Bark/TG/ServerChan/
    // webhook) configured in settings.
    let app2 = app.clone();
    let creds = state.creds();
    let notifier = std::sync::Arc::new(load_notifier(&app));
    let stats_db = stats_db_path(&app);
    let hook_settings = super::config::read_settings(&app);
    let retention = load_retention(&hook_settings);
    let output_root = PathBuf::from(&options.output_dir);
    let marker_dirs = state.marker_dirs.clone();
    tokio::spawn(async move {
        let mut danmaku_log: Option<DanmakuLogHandle> = None;
        while let Some(ev) = rec_rx.recv().await {
            let payload = match ev {
                RecorderEvent::RecordingStarted {
                    room_id,
                    output_dir,
                } => {
                    // Rolling cleanup of old recordings (对标 blrec): runs
                    // when a new session starts, never touches the active one.
                    if !retention.is_noop() {
                        let root = output_root.clone();
                        let policy = retention.clone();
                        let active = output_dir.clone();
                        tokio::task::spawn_blocking(move || {
                            let report = vtb_recorder::retention::run_cleanup(
                                &root,
                                &policy,
                                Some(&active),
                                vtb_recorder::disk::free_space,
                            );
                            if !report.deleted.is_empty() {
                                tracing::info!(
                                    "滚动清理: 删除 {} 个旧录播，释放 {:.1} MB",
                                    report.deleted.len(),
                                    report.freed_bytes as f64 / 1_048_576.0
                                );
                            }
                        });
                    }
                    danmaku_log =
                        start_danmaku_log(room_id, &output_dir, creds.clone(), stats_db.clone());
                    // 打点: this session dir is now the marker target for
                    // the room (hotkey / button / auto alerts append here).
                    marker_dirs
                        .lock()
                        .unwrap()
                        .insert(room_id, output_dir.clone());
                    push_notify(
                        &notifier,
                        vtb_notify::NotifyKind::RecordingStarted,
                        format!("房间 {room_id} 开始录制"),
                        output_dir.to_string_lossy().into_owned(),
                    );
                    if let Some(s) = super::hooks::hook_for(&hook_settings, "recording_started") {
                        super::hooks::run_hook(
                            s,
                            super::hooks::HookEvent {
                                event: "recording_started".into(),
                                room_id,
                                path: Some(output_dir.to_string_lossy().into_owned()),
                                message: None,
                            },
                        );
                    }
                    RecorderPayload::Started {
                        room_id,
                        output_dir: output_dir.to_string_lossy().into_owned(),
                    }
                }
                RecorderEvent::RecordingStopped { room_id, metadata } => {
                    if let Some(h) = danmaku_log.take() {
                        h.stop().await;
                    }
                    marker_dirs.lock().unwrap().remove(&room_id);
                    // FLV tag-level timestamp repair (对标录播姬): fix
                    // server-splice jumps so cutting/subtitles stay aligned.
                    // Clean files are left untouched.
                    for seg in &metadata.segments {
                        let path = seg.path.clone();
                        if path.extension().and_then(|e| e.to_str()) != Some("flv") {
                            continue;
                        }
                        let done = tokio::task::spawn_blocking(move || {
                            let r = vtb_recorder::flv::repair_file(
                                &path,
                                &vtb_recorder::flv::RepairConfig::default(),
                            );
                            (path, r)
                        })
                        .await;
                        if let Ok((path, result)) = done {
                            match result {
                                Ok(s) if s.discontinuities > 0 => tracing::info!(
                                    "FLV 修复 {}: {} 处时间戳跳变已修复 (原文件保留为 .orig)",
                                    path.display(),
                                    s.discontinuities
                                ),
                                Ok(_) => {}
                                Err(e) => tracing::warn!("FLV 修复失败 {}: {e}", path.display()),
                            }
                        }
                    }
                    if let Some(s) = super::hooks::hook_for(&hook_settings, "recording_stopped") {
                        super::hooks::run_hook(
                            s,
                            super::hooks::HookEvent {
                                event: "recording_stopped".into(),
                                room_id,
                                path: metadata
                                    .segments
                                    .first()
                                    .map(|seg| seg.path.to_string_lossy().into_owned()),
                                message: None,
                            },
                        );
                    }
                    push_notify(
                        &notifier,
                        vtb_notify::NotifyKind::RecordingStopped,
                        format!("房间 {room_id} 录制结束"),
                        format!(
                            "{} 段, {:.1} MB",
                            metadata.segments.len(),
                            metadata.total_bytes() as f64 / 1_048_576.0
                        ),
                    );
                    RecorderPayload::Stopped {
                        room_id,
                        total_bytes: metadata.total_bytes(),
                        segments: metadata.segments.len(),
                    }
                }
                RecorderEvent::RecordingError { room_id, message } => {
                    push_notify(
                        &notifier,
                        vtb_notify::NotifyKind::RecordingError,
                        format!("房间 {room_id} 录制异常"),
                        message.clone(),
                    );
                    RecorderPayload::Error { room_id, message }
                }
            };
            let _ = app2.emit(EVENT_RECORDER, &payload);
        }
        if let Some(h) = danmaku_log.take() {
            h.stop().await;
        }
        marker_dirs.lock().unwrap().remove(&room_id);
    });

    state.recorders.lock().unwrap().insert(
        room_id,
        RecorderHandles {
            monitor: monitor_task,
            recorder: recorder_task,
        },
    );
    Ok(())
}

#[tauri::command]
pub async fn recorder_stop(state: State<'_, AppState>, room_id: u64) -> Result<(), String> {
    if let Some(handles) = state.recorders.lock().unwrap().remove(&room_id) {
        // Aborting the monitor closes the channel; the recorder loop then
        // stops any active ffmpeg gracefully and exits.
        handles.monitor.abort();
        Ok(())
    } else {
        Err(format!("room {room_id} not monitored"))
    }
}

#[tauri::command]
pub async fn recorder_status(state: State<'_, AppState>) -> Result<Vec<u64>, String> {
    let map = state.recorders.lock().unwrap();
    Ok(map
        .iter()
        .filter(|(_, h)| !h.monitor.is_finished())
        .map(|(id, _)| *id)
        .collect())
}

#[derive(serde::Serialize)]
pub struct FlvRepairResult {
    pub tags: u64,
    pub discontinuities: u64,
    pub duration_ms: i64,
    pub truncated: bool,
    pub replaced: bool,
}

/// 工具箱: repair timestamps of an existing FLV recording. Clean files
/// are left untouched; repaired files keep the original as `.orig`.
#[tauri::command]
pub async fn flv_repair(path: String) -> Result<FlvRepairResult, String> {
    let p = std::path::PathBuf::from(path);
    let stats = tokio::task::spawn_blocking(move || {
        vtb_recorder::flv::repair_file(&p, &vtb_recorder::flv::RepairConfig::default())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    Ok(FlvRepairResult {
        tags: stats.tags,
        discontinuities: stats.discontinuities,
        duration_ms: stats.last_timestamp_ms,
        truncated: stats.truncated,
        replaced: stats.discontinuities > 0,
    })
}
