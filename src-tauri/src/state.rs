//! Shared application state: running danmaku connections, recorders, jobs.

use std::collections::HashMap;
use std::sync::Mutex;
use tokio::task::JoinHandle;

#[derive(Default)]
pub struct AppState {
    /// room_id → danmaku streaming task.
    pub danmaku: Mutex<HashMap<u64, JoinHandle<()>>>,
    /// room_id → (monitor task, recorder task).
    pub recorders: Mutex<HashMap<u64, RecorderHandles>>,
    /// job id → offline job task.
    pub jobs: Mutex<HashMap<String, JoinHandle<()>>>,
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
            .map(|h| !h.is_finished())
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
}
