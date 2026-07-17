//! Async danmaku client: connects, authenticates, heartbeats, and streams
//! [`LiveEvent`]s over a Tokio mpsc channel.

use crate::api::{BiliApi, DanmakuHost};
use crate::error::Result;
use crate::messages::parse_cmd;
use crate::protocol::{self, Frame, Operation, ProtoVer};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use vtb_common::LiveEvent;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct DanmakuClientConfig {
    pub room_id: u64,
    /// UID to authenticate as; 0 for anonymous.
    pub uid: u64,
    /// buvid3 cookie value, if available (improves reliability in 2024+).
    pub buvid: Option<String>,
    /// Channel buffer size for emitted events.
    pub channel_buffer: usize,
}

impl DanmakuClientConfig {
    pub fn anonymous(room_id: u64) -> Self {
        Self {
            room_id,
            uid: 0,
            buvid: None,
            channel_buffer: 1024,
        }
    }
}

pub struct DanmakuClient {
    config: DanmakuClientConfig,
    api: BiliApi,
}

impl DanmakuClient {
    pub fn new(config: DanmakuClientConfig) -> Result<Self> {
        Ok(Self {
            api: BiliApi::default_client()?,
            config,
        })
    }

    pub fn with_api(config: DanmakuClientConfig, api: BiliApi) -> Self {
        Self { config, api }
    }

    /// Connect and stream events until the connection drops or `stop` is
    /// signalled. Returns the receiver end of the event channel plus a
    /// handle to the running task.
    pub async fn connect(
        self,
    ) -> Result<(mpsc::Receiver<LiveEvent>, tokio::task::JoinHandle<Result<()>>)> {
        let room = self.api.room_init(self.config.room_id).await?;
        let info = self.api.danmu_info(room.real_room_id).await?;
        let host = info.hosts.into_iter().next().ok_or(crate::error::DanmakuError::NoHost)?;

        let (tx, rx) = mpsc::channel(self.config.channel_buffer);
        let room_id = room.real_room_id;
        let uid = self.config.uid;
        let buvid = self.config.buvid.clone();
        let token = info.token;

        let handle = tokio::spawn(async move {
            run_connection(host, room_id, uid, token, buvid, tx).await
        });
        Ok((rx, handle))
    }
}

async fn run_connection(
    host: DanmakuHost,
    room_id: u64,
    uid: u64,
    token: String,
    buvid: Option<String>,
    tx: mpsc::Sender<LiveEvent>,
) -> Result<()> {
    // tokio-tungstenite's rustls backend needs a process-level crypto
    // provider; install ring once (ignore the error if already set).
    static INSTALL_CRYPTO: std::sync::Once = std::sync::Once::new();
    INSTALL_CRYPTO.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });

    let url = host.wss_url();
    let (ws, _resp) = tokio_tungstenite::connect_async(&url).await?;
    let (mut write, mut read) = ws.split();

    // Send auth packet.
    let auth = protocol::encode(
        Operation::Auth,
        ProtoVer::Json,
        &protocol::auth_body(uid, room_id, &token, buvid.as_deref()),
    );
    write.send(Message::Binary(auth)).await?;

    // Heartbeat task.
    let (hb_tx, mut hb_rx) = mpsc::channel::<()>(1);
    let heartbeat_task = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(HEARTBEAT_INTERVAL);
        loop {
            ticker.tick().await;
            if hb_tx.send(()).await.is_err() {
                break;
            }
        }
    });

    loop {
        tokio::select! {
            _ = hb_rx.recv() => {
                if write.send(Message::Binary(protocol::heartbeat())).await.is_err() {
                    break;
                }
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        if dispatch_frames(&data, room_id, &tx).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }

    heartbeat_task.abort();
    Ok(())
}

/// Decode a raw binary WS message and forward any recognized events.
/// Returns `Err` only if the downstream receiver is gone (channel closed).
async fn dispatch_frames(
    data: &[u8],
    room_id: u64,
    tx: &mpsc::Sender<LiveEvent>,
) -> std::result::Result<(), ()> {
    let frames = match protocol::decode_packet(data) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!("failed to decode packet: {e}");
            return Ok(());
        }
    };
    for frame in frames {
        for ev in events_from_frame(&frame, room_id) {
            if tx.send(ev).await.is_err() {
                return Err(());
            }
        }
    }
    Ok(())
}

/// Convert a decoded frame into zero or more events.
pub fn events_from_frame(frame: &Frame, room_id: u64) -> Vec<LiveEvent> {
    match frame.operation {
        Operation::HeartbeatReply => {
            if let Some(count) = protocol::parse_popularity(&frame.body) {
                vec![LiveEvent::WatchedChange {
                    room_id,
                    count: count as u64,
                }]
            } else {
                vec![]
            }
        }
        Operation::Message => {
            match serde_json::from_slice::<serde_json::Value>(&frame.body) {
                Ok(v) => parse_cmd(room_id, &v).into_iter().collect(),
                Err(_) => vec![],
            }
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::encode;

    #[test]
    fn frame_to_danmaku_event() {
        let body = serde_json::json!({
            "cmd": "DANMU_MSG",
            "info": [[0,1,25,0,1700000000000i64],"测试",[1,"u",0],[],[],0,0]
        });
        let frame = Frame {
            operation: Operation::Message,
            proto: ProtoVer::Json,
            body: serde_json::to_vec(&body).unwrap(),
        };
        let events = events_from_frame(&frame, 100);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], LiveEvent::Danmaku(_)));
    }

    #[test]
    fn heartbeat_reply_becomes_watched_change() {
        let frame = Frame {
            operation: Operation::HeartbeatReply,
            proto: ProtoVer::HeartbeatReply,
            body: 777u32.to_be_bytes().to_vec(),
        };
        let events = events_from_frame(&frame, 5);
        assert!(matches!(
            events[0],
            LiveEvent::WatchedChange { count: 777, .. }
        ));
    }

    #[tokio::test]
    async fn dispatch_forwards_events() {
        let (tx, mut rx) = mpsc::channel(8);
        let body = serde_json::json!({
            "cmd": "DANMU_MSG",
            "info": [[0,1,25,0,1700000000000i64],"hi",[1,"u",0],[],[],0,0]
        });
        let packet = encode(
            Operation::Message,
            ProtoVer::Json,
            &serde_json::to_vec(&body).unwrap(),
        );
        dispatch_frames(&packet, 1, &tx).await.unwrap();
        let ev = rx.recv().await.unwrap();
        assert!(matches!(ev, LiveEvent::Danmaku(_)));
    }

    #[tokio::test]
    async fn dispatch_reports_closed_channel() {
        let (tx, rx) = mpsc::channel(8);
        drop(rx);
        let body = serde_json::json!({
            "cmd": "DANMU_MSG",
            "info": [[0,1,25,0,1700000000000i64],"hi",[1,"u",0],[],[],0,0]
        });
        let packet = encode(
            Operation::Message,
            ProtoVer::Json,
            &serde_json::to_vec(&body).unwrap(),
        );
        assert!(dispatch_frames(&packet, 1, &tx).await.is_err());
    }

    #[test]
    fn anonymous_config_defaults() {
        let cfg = DanmakuClientConfig::anonymous(12345);
        assert_eq!(cfg.room_id, 12345);
        assert_eq!(cfg.uid, 0);
        assert!(cfg.buvid.is_none());
    }
}
