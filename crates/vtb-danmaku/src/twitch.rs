//! Twitch chat via anonymous IRC-over-WebSocket (Holodex-style 多平台聊天).
//!
//! Twitch allows read-only anonymous access with a `justinfan*` nick — no
//! OAuth needed. Messages arrive as IRC lines with `twitch.tv/tags`
//! metadata; we normalize PRIVMSG into [`LiveEvent::Danmaku`] so the whole
//! existing pipeline (display, overlay, translation, TTS, highlight)
//! works unchanged.

use crate::error::{DanmakuError, Result};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, watch};
use vtb_common::{DanmakuMsg, LiveEvent};

const TWITCH_IRC_WSS: &str = "wss://irc-ws.chat.twitch.tv:443";

/// Parsed IRC message tags (`@key=value;key2=value2 ...`).
fn parse_tags(raw: &str) -> std::collections::HashMap<&str, &str> {
    raw.split(';')
        .filter_map(|kv| kv.split_once('='))
        .collect()
}

/// IRC tag values escape spaces as `\s` and semicolons as `\:`.
fn unescape_tag(v: &str) -> String {
    v.replace("\\s", " ").replace("\\:", ";").replace("\\\\", "\\")
}

/// What one IRC line means to us.
#[derive(Debug, PartialEq)]
pub enum IrcEvent {
    /// A chat message normalized into the common event model.
    Chat(LiveEvent),
    /// Server keepalive — the client must reply `PONG`.
    Ping(String),
    /// Anything else (JOIN echo, ROOMSTATE, …).
    Other,
}

/// Parse one raw IRC line from Twitch chat.
pub fn parse_irc_line(line: &str) -> IrcEvent {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.is_empty() {
        return IrcEvent::Other;
    }
    if let Some(rest) = line.strip_prefix("PING") {
        return IrcEvent::Ping(rest.trim_start_matches([' ', ':']).to_string());
    }

    // [@tags] :prefix COMMAND #channel :text
    let (tags_raw, rest) = if let Some(t) = line.strip_prefix('@') {
        match t.split_once(' ') {
            Some((tags, rest)) => (tags, rest),
            None => return IrcEvent::Other,
        }
    } else {
        ("", line)
    };
    let mut parts = rest.splitn(2, " PRIVMSG ");
    let prefix = parts.next().unwrap_or("");
    let Some(privmsg) = parts.next() else {
        return IrcEvent::Other;
    };
    let Some((_channel, text)) = privmsg.split_once(" :") else {
        return IrcEvent::Other;
    };

    let tags = parse_tags(tags_raw);
    // Fallback username from the prefix `:nick!nick@nick.tmi.twitch.tv`.
    let nick = prefix
        .trim_start_matches(':')
        .split('!')
        .next()
        .unwrap_or("")
        .to_string();
    let username = tags
        .get("display-name")
        .map(|v| unescape_tag(v))
        .filter(|v| !v.is_empty())
        .unwrap_or(nick);
    let uid = tags
        .get("user-id")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let is_mod = tags.get("mod").map(|v| *v == "1").unwrap_or(false);

    IrcEvent::Chat(LiveEvent::Danmaku(DanmakuMsg {
        room_id: 0, // twitch channels are names, not numeric rooms
        uid,
        username,
        text: text.to_string(),
        timestamp: Utc::now(),
        medal: None,
        guard_level: 0,
        is_admin: is_mod,
        emoticon: None,
    }))
}

/// Connect to a channel's chat anonymously; events stream out of the
/// returned receiver until the watch sender flips to `true` or the socket
/// dies. Reconnection is the caller's job (mirrors `spawn_managed`).
pub fn spawn_twitch_chat(
    channel: &str,
) -> (mpsc::Receiver<LiveEvent>, watch::Sender<bool>) {
    let channel = channel.trim().trim_start_matches('#').to_lowercase();
    let (tx, rx) = mpsc::channel(256);
    let (stop_tx, mut stop_rx) = watch::channel(false);

    tokio::spawn(async move {
        if let Err(e) = run_chat(&channel, tx, &mut stop_rx).await {
            tracing::warn!("twitch chat ({channel}) ended: {e}");
        }
    });
    (rx, stop_tx)
}

async fn run_chat(
    channel: &str,
    tx: mpsc::Sender<LiveEvent>,
    stop: &mut watch::Receiver<bool>,
) -> Result<()> {
    let (ws, _) = tokio_tungstenite::connect_async(TWITCH_IRC_WSS)
        .await
        .map_err(|e| DanmakuError::Api {
            code: -1,
            message: format!("twitch connect: {e}"),
        })?;
    let (mut write, mut read) = ws.split();
    use tokio_tungstenite::tungstenite::Message;

    // Anonymous login + tags capability + join.
    let nick = format!("justinfan{}", std::process::id() % 100_000);
    for line in [
        "CAP REQ :twitch.tv/tags".to_string(),
        format!("NICK {nick}"),
        format!("JOIN #{channel}"),
    ] {
        write
            .send(Message::Text(line.into()))
            .await
            .map_err(|e| DanmakuError::Api {
                code: -1,
                message: format!("twitch send: {e}"),
            })?;
    }

    loop {
        tokio::select! {
            _ = stop.changed() => {
                if *stop.borrow() {
                    return Ok(());
                }
            }
            msg = read.next() => {
                let Some(Ok(msg)) = msg else { return Ok(()) };
                let Ok(text) = msg.into_text() else { continue };
                for line in text.lines() {
                    match parse_irc_line(line) {
                        IrcEvent::Chat(ev) => {
                            if tx.send(ev).await.is_err() {
                                return Ok(());
                            }
                        }
                        IrcEvent::Ping(token) => {
                            let _ = write
                                .send(Message::Text(format!("PONG :{token}").into()))
                                .await;
                        }
                        IrcEvent::Other => {}
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privmsg_with_tags_becomes_danmaku() {
        let line = "@badge-info=;display-name=CoolViewer;mod=1;user-id=1234 \
                    :coolviewer!coolviewer@coolviewer.tmi.twitch.tv PRIVMSG #somestreamer :Hello chat!";
        match parse_irc_line(line) {
            IrcEvent::Chat(LiveEvent::Danmaku(d)) => {
                assert_eq!(d.username, "CoolViewer");
                assert_eq!(d.uid, 1234);
                assert_eq!(d.text, "Hello chat!");
                assert!(d.is_admin);
            }
            other => panic!("expected chat, got {other:?}"),
        }
    }

    #[test]
    fn privmsg_without_tags_uses_nick() {
        let line = ":plainuser!plainuser@x.tmi.twitch.tv PRIVMSG #ch :no tags here";
        match parse_irc_line(line) {
            IrcEvent::Chat(LiveEvent::Danmaku(d)) => {
                assert_eq!(d.username, "plainuser");
                assert_eq!(d.uid, 0);
                assert_eq!(d.text, "no tags here");
            }
            other => panic!("expected chat, got {other:?}"),
        }
    }

    #[test]
    fn message_text_containing_privmsg_keyword_is_safe() {
        let line = ":u!u@x PRIVMSG #ch :I typed PRIVMSG #fake :inside my message";
        match parse_irc_line(line) {
            IrcEvent::Chat(LiveEvent::Danmaku(d)) => {
                assert_eq!(d.text, "I typed PRIVMSG #fake :inside my message");
            }
            other => panic!("expected chat, got {other:?}"),
        }
    }

    #[test]
    fn ping_and_noise() {
        assert_eq!(
            parse_irc_line("PING :tmi.twitch.tv"),
            IrcEvent::Ping("tmi.twitch.tv".into())
        );
        assert_eq!(parse_irc_line(""), IrcEvent::Other);
        assert_eq!(
            parse_irc_line(":tmi.twitch.tv 001 justinfan1 :Welcome, GLHF!"),
            IrcEvent::Other
        );
        assert_eq!(
            parse_irc_line("@emote-only=0 :tmi.twitch.tv ROOMSTATE #ch"),
            IrcEvent::Other
        );
    }

    #[test]
    fn escaped_display_name_unescaped() {
        let line = "@display-name=Name\\sWith\\sSpace;user-id=9 :n!n@x PRIVMSG #c :hi";
        match parse_irc_line(line) {
            IrcEvent::Chat(LiveEvent::Danmaku(d)) => {
                assert_eq!(d.username, "Name With Space");
            }
            other => panic!("expected chat, got {other:?}"),
        }
    }
}
