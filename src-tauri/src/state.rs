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
        }
    }
}

pub struct DanmakuHandles {
    pub pump: JoinHandle<()>,
    pub stop: tokio::sync::watch::Sender<bool>,
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

    /// Snapshot of the current credentials, if logged in.
    pub fn creds(&self) -> Option<Credentials> {
        self.credentials.lock().unwrap().clone()
    }

    /// A reqwest client carrying login cookies when available.
    pub fn api_client(&self) -> Result<reqwest::Client, String> {
        vtb_account::build_client(self.creds().as_ref()).map_err(|e| e.to_string())
    }
}

