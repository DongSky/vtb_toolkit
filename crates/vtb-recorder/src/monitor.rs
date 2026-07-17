//! Open-status monitor: polls a status source and emits transitions
//! (went live / went offline) that drive automatic recording.

use crate::error::Result;
use std::time::Duration;
use tokio::sync::mpsc;
use vtb_common::LiveStatus;

/// Abstracts the source of a room's live status so the monitor can be
/// tested without hitting the network.
#[async_trait::async_trait]
pub trait StatusSource: Send + Sync {
    async fn current_status(&self, room_id: u64) -> Result<LiveStatus>;
}

#[derive(Debug, Clone)]
pub struct MonitorConfig {
    pub room_id: u64,
    pub poll_interval: Duration,
}

impl MonitorConfig {
    pub fn new(room_id: u64) -> Self {
        Self {
            room_id,
            poll_interval: Duration::from_secs(15),
        }
    }
}

/// Emitted when the monitored room's status changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorEvent {
    WentLive { room_id: u64 },
    WentOffline { room_id: u64 },
}

pub struct Monitor<S: StatusSource> {
    config: MonitorConfig,
    source: S,
}

impl<S: StatusSource + 'static> Monitor<S> {
    pub fn new(config: MonitorConfig, source: S) -> Self {
        Self { config, source }
    }

    /// Run the monitor loop, sending events on state transitions. Runs until
    /// the receiver is dropped or `max_ticks` polls have elapsed (None = ∞).
    pub async fn run(
        self,
        tx: mpsc::Sender<MonitorEvent>,
        max_ticks: Option<u64>,
    ) -> Result<()> {
        let mut last = LiveStatus::Offline;
        let mut ticks = 0u64;
        let mut ticker = tokio::time::interval(self.config.poll_interval);
        loop {
            ticker.tick().await;
            let status = match self.source.current_status(self.config.room_id).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("status poll failed: {e}");
                    continue;
                }
            };
            if let Some(event) = transition(last, status, self.config.room_id) {
                if tx.send(event).await.is_err() {
                    break;
                }
            }
            last = status;

            ticks += 1;
            if let Some(m) = max_ticks {
                if ticks >= m {
                    break;
                }
            }
        }
        Ok(())
    }
}

/// Pure transition logic: given previous and current status, what event (if
/// any) should fire. `Round` (轮播) is treated as offline for recording.
pub fn transition(
    prev: LiveStatus,
    curr: LiveStatus,
    room_id: u64,
) -> Option<MonitorEvent> {
    let was_live = prev == LiveStatus::Live;
    let is_live = curr == LiveStatus::Live;
    match (was_live, is_live) {
        (false, true) => Some(MonitorEvent::WentLive { room_id }),
        (true, false) => Some(MonitorEvent::WentOffline { room_id }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn transitions_fire_only_on_change() {
        use LiveStatus::*;
        assert_eq!(
            transition(Offline, Live, 1),
            Some(MonitorEvent::WentLive { room_id: 1 })
        );
        assert_eq!(
            transition(Live, Offline, 1),
            Some(MonitorEvent::WentOffline { room_id: 1 })
        );
        assert_eq!(transition(Live, Live, 1), None);
        assert_eq!(transition(Offline, Offline, 1), None);
        // Round counts as offline.
        assert_eq!(transition(Offline, Round, 1), None);
        assert_eq!(
            transition(Live, Round, 1),
            Some(MonitorEvent::WentOffline { room_id: 1 })
        );
    }

    struct ScriptedSource {
        states: Arc<Mutex<Vec<LiveStatus>>>,
    }

    #[async_trait::async_trait]
    impl StatusSource for ScriptedSource {
        async fn current_status(&self, _room_id: u64) -> Result<LiveStatus> {
            let mut s = self.states.lock().unwrap();
            if s.is_empty() {
                Ok(LiveStatus::Offline)
            } else {
                Ok(s.remove(0))
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn monitor_emits_live_then_offline() {
        let source = ScriptedSource {
            states: Arc::new(Mutex::new(vec![
                LiveStatus::Offline,
                LiveStatus::Live,
                LiveStatus::Live,
                LiveStatus::Offline,
            ])),
        };
        let mut cfg = MonitorConfig::new(42);
        cfg.poll_interval = Duration::from_millis(10);
        let monitor = Monitor::new(cfg, source);
        let (tx, mut rx) = mpsc::channel(8);
        let handle = tokio::spawn(async move { monitor.run(tx, Some(4)).await });

        // Drive the paused clock forward.
        tokio::time::advance(Duration::from_millis(50)).await;

        let mut events = Vec::new();
        while let Some(ev) = rx.recv().await {
            events.push(ev);
        }
        handle.await.unwrap().unwrap();

        assert_eq!(
            events,
            vec![
                MonitorEvent::WentLive { room_id: 42 },
                MonitorEvent::WentOffline { room_id: 42 },
            ]
        );
    }
}
