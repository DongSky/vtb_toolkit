//! Managed danmaku connection: a supervisor that keeps the client
//! connected across network failures with host failover, exponential
//! backoff, and re-handshake (fresh token) on every attempt.
//!
//! Emits both live events and connection-state transitions on a single
//! channel so UIs can show 连接中/已连接/重连中 without extra plumbing.

use crate::api::BiliApi;
use crate::client::{events_from_frame, DanmakuClientConfig};
use crate::error::{DanmakuError, Result};
use crate::protocol::{self, Operation, ProtoVer};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;
use vtb_common::LiveEvent;

/// Everything a managed connection can emit.
#[derive(Debug, Clone, PartialEq)]
pub enum ManagedEvent {
    Live(LiveEvent),
    State(ConnState),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnState {
    /// Handshaking / dialing.
    Connecting { attempt: u32 },
    /// WebSocket up and authenticated.
    Connected { host: String },
    /// Connection ended; the supervisor will retry.
    Disconnected { reason: String },
    /// Sleeping before the next attempt.
    Reconnecting { attempt: u32, delay_ms: u64 },
    /// Supervisor exited (stop requested or receiver dropped).
    Stopped,
}

/// How one connection session ended.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionEnd {
    /// How long the session was connected.
    pub connected_for: Duration,
    pub reason: String,
}

/// One connection lifetime, injectable for supervisor tests.
#[async_trait]
pub trait SessionRunner: Send + Sync {
    /// Handshake + run a single connection until it drops or stop fires.
    /// `attempt` selects the danmaku host (rotation on retries).
    async fn run_session(
        &self,
        config: &DanmakuClientConfig,
        attempt: u32,
        tx: &mpsc::Sender<ManagedEvent>,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<SessionEnd>;
}

/// Exponential backoff with a cap: 1s, 2s, 4s … 60s.
pub fn backoff_delay(attempt: u32) -> Duration {
    let secs = 1u64 << attempt.min(6); // 1..=64
    Duration::from_secs(secs.min(60))
}

/// Sessions at least this long reset the backoff counter.
const STABLE_SESSION: Duration = Duration::from_secs(30);

/// Supervisor loop: keep running sessions until stopped or the receiver
/// goes away. Generic over the runner for testability.
pub async fn supervise<R: SessionRunner>(
    runner: R,
    config: DanmakuClientConfig,
    tx: mpsc::Sender<ManagedEvent>,
    mut stop: watch::Receiver<bool>,
) {
    let mut attempt: u32 = 0;
    loop {
        if *stop.borrow() {
            break;
        }
        if tx
            .send(ManagedEvent::State(ConnState::Connecting { attempt }))
            .await
            .is_err()
        {
            return;
        }

        let stable = match runner.run_session(&config, attempt, &tx, &mut stop).await {
            Ok(end) => {
                let stable = end.connected_for >= STABLE_SESSION;
                if tx
                    .send(ManagedEvent::State(ConnState::Disconnected {
                        reason: end.reason,
                    }))
                    .await
                    .is_err()
                {
                    return;
                }
                stable
            }
            Err(e) => {
                if tx
                    .send(ManagedEvent::State(ConnState::Disconnected {
                        reason: e.to_string(),
                    }))
                    .await
                    .is_err()
                {
                    return;
                }
                false
            }
        };

        if *stop.borrow() {
            break;
        }
        // A stable session restarts the schedule (quick retry, first
        // host); repeated failures back off exponentially.
        attempt = if stable { 0 } else { attempt.saturating_add(1) };
        let delay = backoff_delay(attempt);
        if tx
            .send(ManagedEvent::State(ConnState::Reconnecting {
                attempt,
                delay_ms: delay.as_millis() as u64,
            }))
            .await
            .is_err()
        {
            return;
        }
        // Sleep, but wake immediately if stop is requested.
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = stop.changed() => {}
        }
    }
    let _ = tx.send(ManagedEvent::State(ConnState::Stopped)).await;
}

/// Production session runner: real handshake + WebSocket.
pub struct BiliSessionRunner {
    pub api: BiliApi,
}

#[async_trait]
impl SessionRunner for BiliSessionRunner {
    async fn run_session(
        &self,
        config: &DanmakuClientConfig,
        attempt: u32,
        tx: &mpsc::Sender<ManagedEvent>,
        stop: &mut watch::Receiver<bool>,
    ) -> Result<SessionEnd> {
        // Fresh handshake every attempt: tokens are short-lived and stale
        // ones are the main cause of auth failures after reconnects.
        let room = self.api.room_init(config.room_id).await?;
        let info = self.api.danmu_info(room.real_room_id).await?;
        if info.hosts.is_empty() {
            return Err(DanmakuError::NoHost);
        }
        let host = info.hosts[attempt as usize % info.hosts.len()].clone();

        static INSTALL_CRYPTO: std::sync::Once = std::sync::Once::new();
        INSTALL_CRYPTO.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });

        let (ws, _) = tokio_tungstenite::connect_async(&host.wss_url()).await?;
        let (mut write, mut read) = ws.split();
        let auth = protocol::encode(
            Operation::Auth,
            ProtoVer::Json,
            &protocol::auth_body(
                config.uid,
                room.real_room_id,
                &info.token,
                config.buvid.as_deref(),
            ),
        );
        write.send(Message::Binary(auth)).await?;

        let _ = tx
            .send(ManagedEvent::State(ConnState::Connected {
                host: host.host.clone(),
            }))
            .await;

        let started = tokio::time::Instant::now();
        let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        let reason = 'session: loop {
            tokio::select! {
                _ = stop.changed() => {
                    if *stop.borrow() { break 'session "stop requested".to_string(); }
                }
                _ = heartbeat.tick() => {
                    if write.send(Message::Binary(protocol::heartbeat())).await.is_err() {
                        break 'session "heartbeat send failed".to_string();
                    }
                }
                msg = read.next() => match msg {
                    Some(Ok(Message::Binary(data))) => {
                        match protocol::decode_packet(&data) {
                            Ok(frames) => {
                                for frame in frames {
                                    for ev in events_from_frame(&frame, room.real_room_id) {
                                        if tx.send(ManagedEvent::Live(ev)).await.is_err() {
                                            break 'session "receiver dropped".to_string();
                                        }
                                    }
                                }
                            }
                            Err(e) => tracing::warn!("decode failed: {e}"),
                        }
                    }
                    Some(Ok(Message::Close(_))) => break 'session "server closed".to_string(),
                    Some(Ok(_)) => {}
                    Some(Err(e)) => break 'session format!("ws error: {e}"),
                    None => break 'session "stream ended".to_string(),
                }
            }
        };

        Ok(SessionEnd {
            connected_for: started.elapsed(),
            reason,
        })
    }
}

/// Convenience: spawn a managed connection. Returns the event receiver and
/// a stop handle (set to true to shut down gracefully).
pub fn spawn_managed(
    api: BiliApi,
    config: DanmakuClientConfig,
) -> (mpsc::Receiver<ManagedEvent>, watch::Sender<bool>) {
    let (tx, rx) = mpsc::channel(config.channel_buffer);
    let (stop_tx, stop_rx) = watch::channel(false);
    tokio::spawn(supervise(BiliSessionRunner { api }, config, tx, stop_rx));
    (rx, stop_tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff_delay(0), Duration::from_secs(1));
        assert_eq!(backoff_delay(1), Duration::from_secs(2));
        assert_eq!(backoff_delay(3), Duration::from_secs(8));
        assert_eq!(backoff_delay(6), Duration::from_secs(60)); // 64→cap 60
        assert_eq!(backoff_delay(99), Duration::from_secs(60));
    }

    /// Scripted runner: each call pops the next outcome.
    struct ScriptedRunner {
        outcomes: std::sync::Mutex<Vec<Result<SessionEnd>>>,
        calls: Arc<AtomicU32>,
        attempts_seen: Arc<std::sync::Mutex<Vec<u32>>>,
    }

    #[async_trait]
    impl SessionRunner for ScriptedRunner {
        async fn run_session(
            &self,
            _config: &DanmakuClientConfig,
            attempt: u32,
            tx: &mpsc::Sender<ManagedEvent>,
            _stop: &mut watch::Receiver<bool>,
        ) -> Result<SessionEnd> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.attempts_seen.lock().unwrap().push(attempt);
            // Take the next outcome without holding the lock across awaits.
            let next = {
                let mut outcomes = self.outcomes.lock().unwrap();
                if outcomes.is_empty() {
                    None
                } else {
                    Some(outcomes.remove(0))
                }
            };
            match next {
                Some(outcome) => outcome,
                None => {
                    // Emit one event then pretend a long stable session.
                    let _ = tx
                        .send(ManagedEvent::Live(LiveEvent::WatchedChange {
                            room_id: 1,
                            count: 1,
                        }))
                        .await;
                    Ok(SessionEnd {
                        connected_for: Duration::from_secs(60),
                        reason: "scripted end".into(),
                    })
                }
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn reconnects_after_failure_with_backoff() {
        let runner = ScriptedRunner {
            outcomes: std::sync::Mutex::new(vec![
                Err(DanmakuError::NoHost),
                Err(DanmakuError::NoHost),
            ]),
            calls: Arc::new(AtomicU32::new(0)),
            attempts_seen: Arc::default(),
        };
        let calls = runner.calls.clone();
        let (tx, mut rx) = mpsc::channel(64);
        let (stop_tx, stop_rx) = watch::channel(false);
        let handle = tokio::spawn(supervise(
            runner,
            DanmakuClientConfig::anonymous(1),
            tx,
            stop_rx,
        ));

        // Collect events while auto-advancing paused time.
        let mut states = Vec::new();
        while calls.load(Ordering::SeqCst) < 3 {
            tokio::select! {
                ev = rx.recv() => {
                    if let Some(ManagedEvent::State(s)) = ev { states.push(s); }
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
        }
        stop_tx.send(true).unwrap();
        while let Some(ev) = rx.recv().await {
            if let ManagedEvent::State(s) = ev {
                states.push(s);
            }
        }
        handle.await.unwrap();

        // Should show: connect fail → reconnect wait → connect fail →
        // reconnect wait → third session runs.
        let reconnects: Vec<_> = states
            .iter()
            .filter_map(|s| match s {
                ConnState::Reconnecting { attempt, delay_ms } => Some((*attempt, *delay_ms)),
                _ => None,
            })
            .collect();
        assert!(reconnects.len() >= 2, "states: {states:?}");
        // Backoff grows between consecutive failures.
        assert!(reconnects[1].1 > reconnects[0].1);
        assert!(matches!(states.last(), Some(ConnState::Stopped)));
    }

    #[tokio::test(start_paused = true)]
    async fn stable_session_resets_attempt() {
        let runner = ScriptedRunner {
            outcomes: std::sync::Mutex::new(vec![
                Err(DanmakuError::NoHost),
                // Stable session (60s) — attempt must reset to 0 after it.
                Ok(SessionEnd {
                    connected_for: Duration::from_secs(60),
                    reason: "x".into(),
                }),
            ]),
            calls: Arc::new(AtomicU32::new(0)),
            attempts_seen: Arc::default(),
        };
        let calls = runner.calls.clone();
        let seen_ref = runner.attempts_seen.clone();

        let (tx, mut rx) = mpsc::channel(64);
        let (stop_tx, stop_rx) = watch::channel(false);
        let handle = tokio::spawn(supervise(
            runner,
            DanmakuClientConfig::anonymous(1),
            tx,
            stop_rx,
        ));
        while calls.load(Ordering::SeqCst) < 3 {
            tokio::select! {
                _ = rx.recv() => {}
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
        }
        stop_tx.send(true).unwrap();
        while rx.recv().await.is_some() {}
        handle.await.unwrap();

        let seen = seen_ref.lock().unwrap().clone();
        // Call 1: attempt 0 (fails) → call 2: attempt 1 (stable) →
        // call 3: attempt 0 again (reset).
        assert_eq!(seen[0], 0);
        assert_eq!(seen[1], 1);
        assert_eq!(seen[2], 0, "attempt should reset after stable session");
    }

    #[tokio::test(start_paused = true)]
    async fn stop_ends_supervisor_promptly() {
        let runner = ScriptedRunner {
            outcomes: std::sync::Mutex::new(vec![]),
            calls: Arc::new(AtomicU32::new(0)),
            attempts_seen: Arc::default(),
        };
        let (tx, mut rx) = mpsc::channel(64);
        let (stop_tx, stop_rx) = watch::channel(false);
        let handle = tokio::spawn(supervise(
            runner,
            DanmakuClientConfig::anonymous(1),
            tx,
            stop_rx,
        ));
        // Let one session run, then stop during the backoff sleep.
        tokio::time::sleep(Duration::from_millis(50)).await;
        stop_tx.send(true).unwrap();
        let mut got_stopped = false;
        while let Some(ev) = rx.recv().await {
            if matches!(ev, ManagedEvent::State(ConnState::Stopped)) {
                got_stopped = true;
            }
        }
        handle.await.unwrap();
        assert!(got_stopped);
    }
}
