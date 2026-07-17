//! Push notifications for stream events (开播 / 录制完成 / 出错).
//!
//! Four delivery channels — Bark (iOS), Server 酱, Telegram bot and generic
//! JSON webhooks — behind one [`Notifier`] trait. Request construction is
//! pure (unit-testable, see each notifier's `build_*` method) and separated
//! from sending; [`MultiNotifier`] fans an event out to every configured
//! channel concurrently, and [`NotifyConfig`] builds one from user settings.

mod config;
mod error;
mod multi;
mod notifiers;

pub use config::{NotifyConfig, TelegramConfig};
pub use error::{NotifyError, Result};
pub use multi::MultiNotifier;
pub use notifiers::{Bark, ServerChan, Telegram, Webhook};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyKind {
    LiveStart,
    RecordingStarted,
    RecordingStopped,
    RecordingError,
    Highlight,
    Custom,
}

/// A notification ready for delivery.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotifyEvent {
    pub title: String,
    pub body: String,
    pub kind: NotifyKind,
}

impl NotifyEvent {
    pub fn new(kind: NotifyKind, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            kind,
        }
    }
}

/// A delivery channel.
#[async_trait]
pub trait Notifier: Send + Sync {
    /// Deliver one event.
    async fn notify(&self, ev: &NotifyEvent) -> Result<()>;
    /// Short channel name for logs ("bark", "telegram", …).
    fn name(&self) -> &str;
}

/// HTTP client used by every notifier: 10 s overall timeout.
pub(crate) fn default_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("reqwest client construction cannot fail with these options")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Bytes;
    use axum::extract::State;
    use axum::http::{HeaderMap, Method, Uri};
    use axum::Router;
    use tokio::sync::mpsc;

    #[derive(Debug)]
    struct Captured {
        method: String,
        path: String,
        content_type: String,
        body: Vec<u8>,
    }

    async fn capture(
        State(tx): State<mpsc::Sender<Captured>>,
        method: Method,
        uri: Uri,
        headers: HeaderMap,
        body: Bytes,
    ) -> &'static str {
        let _ = tx
            .send(Captured {
                method: method.to_string(),
                path: uri.path().to_string(),
                content_type: headers
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .to_string(),
                body: body.to_vec(),
            })
            .await;
        "ok"
    }

    /// 127.0.0.1 test server that captures every request (any path/method).
    async fn spawn_capture_server() -> (String, mpsc::Receiver<Captured>) {
        let (tx, rx) = mpsc::channel(16);
        let app = Router::new().fallback(capture).with_state(tx);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}"), rx)
    }

    async fn recv(rx: &mut mpsc::Receiver<Captured>) -> Captured {
        tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("request arrives timely")
            .expect("capture channel open")
    }

    #[test]
    fn kind_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&NotifyKind::LiveStart).unwrap(),
            "\"live_start\""
        );
        assert_eq!(
            serde_json::to_string(&NotifyKind::RecordingError).unwrap(),
            "\"recording_error\""
        );
        let ev = NotifyEvent::new(NotifyKind::RecordingStopped, "t", "b");
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["kind"], "recording_stopped");
        let back: NotifyEvent = serde_json::from_value(v).unwrap();
        assert_eq!(back, ev);
    }

    #[tokio::test]
    async fn webhook_posts_full_event_json() {
        let (base, mut rx) = spawn_capture_server().await;
        let hook = Webhook::new(format!("{base}/hooks/vtb"));
        let ev = NotifyEvent::new(NotifyKind::LiveStart, "开播啦", "标题《测试 100%》");
        hook.notify(&ev).await.unwrap();

        let got = recv(&mut rx).await;
        assert_eq!(got.method, "POST");
        assert_eq!(got.path, "/hooks/vtb");
        assert!(got.content_type.starts_with("application/json"));
        let v: serde_json::Value = serde_json::from_slice(&got.body).unwrap();
        assert_eq!(v["title"], "开播啦");
        assert_eq!(v["body"], "标题《测试 100%》");
        assert_eq!(v["kind"], "live_start");
    }

    #[tokio::test]
    async fn bark_sends_get_with_encoded_path() {
        let (base, mut rx) = spawn_capture_server().await;
        let bark = Bark::with_server("KEY", &base);
        let ev = NotifyEvent::new(NotifyKind::LiveStart, "开播", "主播 A/B");
        bark.notify(&ev).await.unwrap();

        let got = recv(&mut rx).await;
        assert_eq!(got.method, "GET");
        assert_eq!(got.path, "/KEY/%E5%BC%80%E6%92%AD/%E4%B8%BB%E6%92%AD%20A%2FB");
        assert!(got.body.is_empty());
    }

    #[tokio::test]
    async fn multi_counts_only_reachable_channels() {
        let (base, mut rx) = spawn_capture_server().await;
        // Grab an ephemeral port and close it: connecting there is refused.
        let dead = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let dead_addr = dead.local_addr().unwrap();
        drop(dead);

        let multi = MultiNotifier(vec![
            Box::new(Webhook::new(format!("{base}/ok"))),
            Box::new(Webhook::new(format!("http://{dead_addr}/dead"))),
        ]);
        let ev = NotifyEvent::new(NotifyKind::RecordingStopped, "录制完成", "文件已保存");
        assert_eq!(multi.notify_all(&ev).await, 1);
        let got = recv(&mut rx).await;
        assert_eq!(got.path, "/ok");
    }
}
