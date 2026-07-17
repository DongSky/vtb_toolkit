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
    Started { room_id: u64, output_dir: String },
    Stopped { room_id: u64, total_bytes: u64, segments: usize },
    Error { room_id: u64, message: String },
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

/// Record every live event of `room_id` into `<dir>/danmaku.jsonl` until
/// stopped (managed connection: reconnects on its own).
fn start_danmaku_log(
    room_id: u64,
    dir: &std::path::Path,
    creds: Option<vtb_account::Credentials>,
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
    let task = tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            match ev {
                vtb_danmaku::ManagedEvent::Live(live) => {
                    let entry = LogEntry {
                        received_at: chrono::Utc::now(),
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
    let auto = AutoRecorder::new(
        {
            let mut cfg = AutoRecorderConfig::new(room_id, PathBuf::from(&options.output_dir));
            cfg.segment = segment;
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

    // Pump recorder events to the frontend, and keep a danmaku JSONL
    // logger running for the lifetime of each recording session so the
    // offline pipeline gets highlight signals for free.
    let app2 = app.clone();
    let creds = state.creds();
    tokio::spawn(async move {
        let mut danmaku_log: Option<DanmakuLogHandle> = None;
        while let Some(ev) = rec_rx.recv().await {
            let payload = match ev {
                RecorderEvent::RecordingStarted { room_id, output_dir } => {
                    danmaku_log = start_danmaku_log(room_id, &output_dir, creds.clone());
                    RecorderPayload::Started {
                        room_id,
                        output_dir: output_dir.to_string_lossy().into_owned(),
                    }
                }
                RecorderEvent::RecordingStopped { room_id, metadata } => {
                    if let Some(h) = danmaku_log.take() {
                        h.stop().await;
                    }
                    RecorderPayload::Stopped {
                        room_id,
                        total_bytes: metadata.total_bytes(),
                        segments: metadata.segments.len(),
                    }
                }
                RecorderEvent::RecordingError { room_id, message } => {
                    RecorderPayload::Error { room_id, message }
                }
            };
            let _ = app2.emit(EVENT_RECORDER, &payload);
        }
        if let Some(h) = danmaku_log.take() {
            h.stop().await;
        }
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
pub async fn recorder_stop(
    state: State<'_, AppState>,
    room_id: u64,
) -> Result<(), String> {
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
