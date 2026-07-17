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

    // Pump recorder events to the frontend.
    let app2 = app.clone();
    tokio::spawn(async move {
        while let Some(ev) = rec_rx.recv().await {
            let payload = match ev {
                RecorderEvent::RecordingStarted { room_id, output_dir } => {
                    RecorderPayload::Started {
                        room_id,
                        output_dir: output_dir.to_string_lossy().into_owned(),
                    }
                }
                RecorderEvent::RecordingStopped { room_id, metadata } => {
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
