//! Shared application state: running danmaku connections, recorders, jobs.

use std::collections::HashMap;
use std::sync::Mutex;
use tokio::task::JoinHandle;
use vtb_account::{Credentials, QrLogin};

pub struct AppState {
    /// room_id → managed danmaku connection.
    pub danmaku: Mutex<HashMap<u64, DanmakuHandles>>,
    /// room_id → (monitor task, recorder task).
    pub recorders: Mutex<HashMap<u64, RecorderHandles>>,
    /// job id → offline job task.
    pub jobs: Mutex<HashMap<String, JoinHandle<()>>>,
    /// room_id → live subtitle task.
    pub subtitles: Mutex<HashMap<u64, JoinHandle<()>>>,
    /// Logged-in credentials (loaded from keychain at startup).
    pub credentials: Mutex<Option<Credentials>>,
    /// In-progress QR login session.
    pub qr_login: tokio::sync::Mutex<Option<QrLogin>>,
    /// OBS overlay: publisher lives for the whole app; server on demand.
    pub overlay_publisher: vtb_overlay::OverlayPublisher,
    pub overlay: tokio::sync::Mutex<Option<vtb_overlay::OverlayServer>>,
    /// Danmaku auto-translation worker (blivechat-style). Shared with the
    /// danmaku pumps via Arc so start/stop applies to live connections.
    pub danmaku_translate: std::sync::Arc<Mutex<Option<DanmakuTranslateHandle>>>,
    /// 场控: auto-thank config (shared with pumps), serial send worker and
    /// per-room timed announcements.
    pub autothank: crate::commands::danmaku_send::SharedAutoThank,
    pub danmaku_send_tx:
        std::sync::OnceLock<tokio::sync::mpsc::Sender<(u64, String)>>,
    pub danmaku_timers: Mutex<HashMap<u64, JoinHandle<()>>>,
    /// Danmaku TTS switches + speech queue (Arc so pump tasks can hold them).
    pub tts_enabled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub tts_paid_only: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub tts_tx: std::sync::OnceLock<tokio::sync::mpsc::Sender<String>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            danmaku: Mutex::default(),
            recorders: Mutex::default(),
            jobs: Mutex::default(),
            subtitles: Mutex::default(),
            credentials: Mutex::default(),
            qr_login: tokio::sync::Mutex::default(),
            overlay_publisher: vtb_overlay::publisher(),
            overlay: tokio::sync::Mutex::default(),
            danmaku_translate: std::sync::Arc::new(Mutex::new(None)),
            autothank: std::sync::Arc::new(Mutex::new(Default::default())),
            danmaku_send_tx: std::sync::OnceLock::new(),
            danmaku_timers: Mutex::default(),
            tts_enabled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            tts_paid_only: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            tts_tx: std::sync::OnceLock::new(),
        }
    }
}

pub struct DanmakuHandles {
    pub pump: JoinHandle<()>,
    pub stop: tokio::sync::watch::Sender<bool>,
}

/// Running danmaku translation worker. Pumps `try_send` jobs into `tx`;
/// the worker translates serially and publishes results.
pub struct DanmakuTranslateHandle {
    pub tx: tokio::sync::mpsc::Sender<(u64, String)>,
    pub task: JoinHandle<()>,
    pub target_lang: String,
}

pub struct RecorderHandles {
    pub monitor: JoinHandle<()>,
    pub recorder: JoinHandle<()>,
}

impl AppState {
    pub fn danmaku_connected(&self, room_id: u64) -> bool {
        self.danmaku
            .lock()
            .unwrap()
            .get(&room_id)
            .map(|h| !h.pump.is_finished())
            .unwrap_or(false)
    }

    pub fn recorder_active(&self, room_id: u64) -> bool {
        self.recorders
            .lock()
            .unwrap()
            .get(&room_id)
            .map(|h| !h.monitor.is_finished())
            .unwrap_or(false)
    }

    /// Live snapshot handles for the REST API (cheap to clone into
    /// closures; refreshed lazily on read).
    pub fn danmaku_rooms_snapshot(&self) -> std::sync::Arc<Mutex<Vec<u64>>> {
        let out = std::sync::Arc::new(Mutex::new(vec![]));
        *out.lock().unwrap() = self
            .danmaku
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, h)| !h.pump.is_finished())
            .map(|(id, _)| *id)
            .collect();
        out
    }

    pub fn recorder_rooms_snapshot(&self) -> std::sync::Arc<Mutex<Vec<u64>>> {
        let out = std::sync::Arc::new(Mutex::new(vec![]));
        *out.lock().unwrap() = self
            .recorders
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, h)| !h.monitor.is_finished())
            .map(|(id, _)| *id)
            .collect();
        out
    }

    /// Snapshot of the current credentials, if logged in.
    pub fn creds(&self) -> Option<Credentials> {
        self.credentials.lock().unwrap().clone()
    }

    /// A reqwest client carrying login cookies when available.
    pub fn api_client(&self) -> Result<reqwest::Client, String> {
        vtb_account::build_client(self.creds().as_ref()).map_err(|e| e.to_string())
    }
}

