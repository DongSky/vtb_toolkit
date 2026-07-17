//! The four delivery channels.
//!
//! Each notifier separates *pure request construction* (`build_url` /
//! `build_request`, unit-testable without a network) from actual sending
//! (the [`Notifier`] impl).

use crate::{default_client, NotifyEvent, Notifier, Result};
use async_trait::async_trait;
use std::fmt::Write as _;

pub(crate) const DEFAULT_BARK_SERVER: &str = "https://api.day.app";

/// Percent-encode one URL path segment (RFC 3986: unreserved bytes kept,
/// everything else — including `/`, spaces and non-ASCII UTF-8 — escaped).
pub(crate) fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// [Bark](https://bark.day.app) — iOS push via `GET {server}/{key}/{title}/{body}`.
pub struct Bark {
    key: String,
    server: String,
    client: reqwest::Client,
}

impl Bark {
    /// Bark on the public `https://api.day.app` server.
    pub fn new(key: impl Into<String>) -> Self {
        Self::with_server(key, DEFAULT_BARK_SERVER)
    }

    /// Bark on a self-hosted server (trailing `/` is trimmed).
    pub fn with_server(key: impl Into<String>, server: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            server: server.into().trim_end_matches('/').to_string(),
            client: default_client(),
        }
    }

    /// Pure: the full GET URL with title/body as encoded path segments.
    pub fn build_url(&self, ev: &NotifyEvent) -> String {
        format!(
            "{}/{}/{}/{}",
            self.server,
            self.key,
            encode_path_segment(&ev.title),
            encode_path_segment(&ev.body)
        )
    }
}

#[async_trait]
impl Notifier for Bark {
    async fn notify(&self, ev: &NotifyEvent) -> Result<()> {
        self.client
            .get(self.build_url(ev))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    fn name(&self) -> &str {
        "bark"
    }
}

/// [Server 酱](https://sct.ftqq.com) — WeChat push via form POST.
pub struct ServerChan {
    key: String,
    client: reqwest::Client,
}

impl ServerChan {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            client: default_client(),
        }
    }

    /// Pure: `(url, form pairs)` for `POST https://sctapi.ftqq.com/{key}.send`.
    pub fn build_request(&self, ev: &NotifyEvent) -> (String, [(&'static str, String); 2]) {
        (
            format!("https://sctapi.ftqq.com/{}.send", self.key),
            [("title", ev.title.clone()), ("desp", ev.body.clone())],
        )
    }
}

#[async_trait]
impl Notifier for ServerChan {
    async fn notify(&self, ev: &NotifyEvent) -> Result<()> {
        let (url, form) = self.build_request(ev);
        self.client
            .post(url)
            .form(&form)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    fn name(&self) -> &str {
        "serverchan"
    }
}

/// Telegram bot — `sendMessage` with `"title\n\nbody"` as text.
pub struct Telegram {
    bot_token: String,
    chat_id: String,
    client: reqwest::Client,
}

impl Telegram {
    pub fn new(bot_token: impl Into<String>, chat_id: impl Into<String>) -> Self {
        Self {
            bot_token: bot_token.into(),
            chat_id: chat_id.into(),
            client: default_client(),
        }
    }

    /// Pure: `(url, JSON payload)` for the Bot API `sendMessage` call.
    pub fn build_request(&self, ev: &NotifyEvent) -> (String, serde_json::Value) {
        (
            format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token),
            serde_json::json!({
                "chat_id": self.chat_id,
                "text": format!("{}\n\n{}", ev.title, ev.body),
            }),
        )
    }
}

#[async_trait]
impl Notifier for Telegram {
    async fn notify(&self, ev: &NotifyEvent) -> Result<()> {
        let (url, payload) = self.build_request(ev);
        self.client
            .post(url)
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    fn name(&self) -> &str {
        "telegram"
    }
}

/// Generic webhook — POSTs the whole [`NotifyEvent`] as JSON.
pub struct Webhook {
    url: String,
    client: reqwest::Client,
}

impl Webhook {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            client: default_client(),
        }
    }

    /// Pure: `(url, JSON payload)` — the payload is the serialized event.
    pub fn build_request(&self, ev: &NotifyEvent) -> (String, serde_json::Value) {
        (
            self.url.clone(),
            serde_json::to_value(ev).unwrap_or(serde_json::Value::Null),
        )
    }
}

#[async_trait]
impl Notifier for Webhook {
    async fn notify(&self, ev: &NotifyEvent) -> Result<()> {
        let (url, payload) = self.build_request(ev);
        self.client
            .post(url)
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    fn name(&self) -> &str {
        "webhook"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NotifyKind;

    fn ev(title: &str, body: &str) -> NotifyEvent {
        NotifyEvent::new(NotifyKind::LiveStart, title, body)
    }

    #[test]
    fn encode_keeps_unreserved_ascii() {
        assert_eq!(encode_path_segment("abc-XYZ_0.9~"), "abc-XYZ_0.9~");
    }

    #[test]
    fn encode_escapes_chinese_and_special_chars() {
        assert_eq!(encode_path_segment("开播"), "%E5%BC%80%E6%92%AD");
        assert_eq!(
            encode_path_segment("a b/c?d#e%f&g"),
            "a%20b%2Fc%3Fd%23e%25f%26g"
        );
    }

    #[test]
    fn bark_url_uses_default_server_and_encodes_segments() {
        let bark = Bark::new("KEY123");
        let url = bark.build_url(&ev("开播", "主播 A/B"));
        assert_eq!(
            url,
            "https://api.day.app/KEY123/%E5%BC%80%E6%92%AD/%E4%B8%BB%E6%92%AD%20A%2FB"
        );
    }

    #[test]
    fn bark_custom_server_trailing_slash_trimmed() {
        let bark = Bark::with_server("k", "http://127.0.0.1:9/");
        assert_eq!(bark.build_url(&ev("t", "b")), "http://127.0.0.1:9/k/t/b");
    }

    #[test]
    fn serverchan_posts_title_and_desp_form() {
        let sc = ServerChan::new("SCT123ABC");
        let (url, form) = sc.build_request(&ev("直播开始", "标题:《测试》"));
        assert_eq!(url, "https://sctapi.ftqq.com/SCT123ABC.send");
        assert_eq!(form[0], ("title", "直播开始".to_string()));
        assert_eq!(form[1], ("desp", "标题:《测试》".to_string()));
    }

    #[test]
    fn telegram_payload_joins_title_and_body() {
        let tg = Telegram::new("123:ABC-def", "-1001234");
        let (url, payload) = tg.build_request(&ev("开播啦", "快来看"));
        assert_eq!(url, "https://api.telegram.org/bot123:ABC-def/sendMessage");
        assert_eq!(payload["chat_id"], "-1001234");
        assert_eq!(payload["text"], "开播啦\n\n快来看");
    }

    #[test]
    fn webhook_payload_is_whole_event() {
        let wh = Webhook::new("http://example.com/hook");
        let (url, payload) = wh.build_request(&ev("开播", "身体"));
        assert_eq!(url, "http://example.com/hook");
        assert_eq!(
            payload,
            serde_json::json!({"title": "开播", "body": "身体", "kind": "live_start"})
        );
    }
}
