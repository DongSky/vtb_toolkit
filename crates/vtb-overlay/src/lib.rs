//! OBS browser-source overlay server.
//!
//! A tiny local HTTP+WebSocket server that OBS (or any browser) points at:
//! - `/overlay/danmaku`  — transparent chat overlay
//! - `/overlay/subtitle` — bottom subtitle bar (原文 + 译文)
//! - `/ws`               — event stream feeding both pages
//!
//! The app pushes [`OverlayMessage`]s into a broadcast channel; every
//! connected page receives them as JSON. The server binds 127.0.0.1 only.

mod pages;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::sync::broadcast;
use vtb_common::LiveEvent;

/// Everything an overlay page can render.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OverlayMessage {
    Danmaku(LiveEvent),
    Subtitle {
        text: String,
        translated: Option<String>,
        #[serde(default)]
        lang: Option<String>,
    },
    /// Clears the subtitle bar (e.g. stream stopped).
    SubtitleClear,
}

/// Cloneable publisher handle; safe to call whether or not the server is
/// running (messages are dropped when nobody listens).
#[derive(Clone)]
pub struct OverlayPublisher {
    tx: broadcast::Sender<OverlayMessage>,
}

impl OverlayPublisher {
    pub fn publish(&self, msg: OverlayMessage) {
        let _ = self.tx.send(msg);
    }

    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

/// A running overlay server.
pub struct OverlayServer {
    pub port: u16,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl OverlayServer {
    /// URLs for OBS browser sources.
    pub fn danmaku_url(&self) -> String {
        format!("http://127.0.0.1:{}/overlay/danmaku", self.port)
    }

    pub fn subtitle_url(&self) -> String {
        format!("http://127.0.0.1:{}/overlay/subtitle", self.port)
    }

    /// Stop the server.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), self.task).await;
    }
}

#[derive(Clone)]
struct AppCtx {
    tx: broadcast::Sender<OverlayMessage>,
}

/// Create the publisher used by the app regardless of server lifetime.
pub fn publisher() -> OverlayPublisher {
    let (tx, _) = broadcast::channel(512);
    OverlayPublisher { tx }
}

/// Start the server on 127.0.0.1 (port 0 = ephemeral). The `publisher`'s
/// messages are streamed to all connected overlay pages.
pub async fn start(
    publisher: &OverlayPublisher,
    port: u16,
) -> std::io::Result<OverlayServer> {
    let ctx = AppCtx {
        tx: publisher.tx.clone(),
    };
    let app = Router::new()
        .route("/overlay/danmaku", get(danmaku_page))
        .route("/overlay/subtitle", get(subtitle_page))
        .route("/ws", get(ws_handler))
        .with_state(ctx);

    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let port = listener.local_addr()?.port();

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        });
        if let Err(e) = server.await {
            tracing::warn!("overlay server error: {e}");
        }
    });

    Ok(OverlayServer {
        port,
        shutdown: Some(shutdown_tx),
        task,
    })
}

async fn danmaku_page() -> Html<&'static str> {
    Html(pages::DANMAKU_HTML)
}

async fn subtitle_page() -> Html<&'static str> {
    Html(pages::SUBTITLE_HTML)
}

async fn ws_handler(ws: WebSocketUpgrade, State(ctx): State<AppCtx>) -> axum::response::Response {
    ws.on_upgrade(move |socket| client_loop(socket, ctx.tx.subscribe()))
}

async fn client_loop(mut socket: WebSocket, mut rx: broadcast::Receiver<OverlayMessage>) {
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(m) => {
                    let json = match serde_json::to_string(&m) {
                        Ok(j) => j,
                        Err(_) => continue,
                    };
                    if socket.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::debug!("overlay client lagged, skipped {n}");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            // Drain (and ignore) anything the page sends; detect close.
            incoming = socket.recv() => match incoming {
                Some(Ok(_)) => {}
                _ => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn serves_pages_and_streams_events() {
        let publisher = publisher();
        let server = start(&publisher, 0).await.unwrap();
        let port = server.port;

        // Pages exist and carry the WS bootstrap.
        let danmaku = reqwest::get(format!("http://127.0.0.1:{port}/overlay/danmaku"))
            .await
            .unwrap();
        assert_eq!(danmaku.status(), 200);
        let body = danmaku.text().await.unwrap();
        assert!(body.contains("WebSocket"));
        assert!(body.contains("background: transparent") || body.contains("background:transparent"));

        let subtitle = reqwest::get(format!("http://127.0.0.1:{port}/overlay/subtitle"))
            .await
            .unwrap();
        assert_eq!(subtitle.status(), 200);

        // Connect WS, publish, receive.
        let (ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
            .await
            .unwrap();
        let (_write, mut read) = ws.split();
        // Give the subscription a beat to register.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        publisher.publish(OverlayMessage::Subtitle {
            text: "こんばんは".into(),
            translated: Some("晚上好".into()),
            lang: Some("ja".into()),
        });

        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), read.next())
            .await
            .expect("ws message timely")
            .expect("stream open")
            .expect("no ws error");
        let text = msg.into_text().unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["type"], "subtitle");
        assert_eq!(v["translated"], "晚上好");

        server.shutdown().await;
        // Port should be released; a new bind on it succeeds.
        let rebind = tokio::net::TcpListener::bind(("127.0.0.1", port)).await;
        assert!(rebind.is_ok());
    }

    #[tokio::test]
    async fn publish_without_listeners_is_noop() {
        let publisher = publisher();
        publisher.publish(OverlayMessage::SubtitleClear);
        assert_eq!(publisher.receiver_count(), 0);
    }

    #[tokio::test]
    async fn danmaku_events_roundtrip_serialization() {
        use chrono::Utc;
        use vtb_common::DanmakuMsg;
        let msg = OverlayMessage::Danmaku(LiveEvent::Danmaku(DanmakuMsg {
            room_id: 1,
            uid: 2,
            username: "观众".into(),
            text: "hello".into(),
            timestamp: Utc::now(),
            medal: None,
            guard_level: 0,
            is_admin: false,
            emoticon: None,
        }));
        let json = serde_json::to_string(&msg).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "danmaku");
        assert_eq!(v["kind"], "danmaku");
        assert_eq!(v["username"], "观众");
    }
}
