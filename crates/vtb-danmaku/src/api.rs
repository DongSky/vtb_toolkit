//! HTTP handshake with Bilibili to obtain danmaku server host + token.
//!
//! Flow:
//! 1. `GET /room/v1/Room/room_init?id={room}` → real room id + live status.
//! 2. `GET /xlive/web-room/v1/index/getDanmuInfo?id={room}` → token + host list.
//!
//! The `getDanmuInfo` endpoint requires a signed request (WBI) in 2024+.
//! We keep the signing pluggable so a cookie-authenticated caller can pass
//! a pre-signed URL, but provide the default unsigned request for anon use
//! (which still works for many rooms at lower rate limits).

use crate::error::{DanmakuError, Result};
use serde::Deserialize;
use vtb_common::LiveStatus;

const ROOM_INIT: &str = "https://api.live.bilibili.com/room/v1/Room/room_init";
const DANMU_INFO: &str =
    "https://api.live.bilibili.com/xlive/web-room/v1/index/getDanmuInfo";

/// A danmaku WebSocket host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DanmakuHost {
    pub host: String,
    pub wss_port: u16,
}

impl DanmakuHost {
    pub fn wss_url(&self) -> String {
        format!("wss://{}:{}/sub", self.host, self.wss_port)
    }
}

/// Result of the room handshake.
#[derive(Debug, Clone)]
pub struct RoomInfo {
    pub real_room_id: u64,
    pub status: LiveStatus,
}

/// Result of `getDanmuInfo`.
#[derive(Debug, Clone)]
pub struct DanmuInfo {
    pub token: String,
    pub hosts: Vec<DanmakuHost>,
}

#[derive(Deserialize)]
struct ApiEnvelope<T> {
    code: i64,
    #[serde(default)]
    message: String,
    data: Option<T>,
}

#[derive(Deserialize)]
struct RoomInitData {
    room_id: u64,
    live_status: u8,
}

#[derive(Deserialize)]
struct DanmuInfoData {
    token: String,
    host_list: Vec<HostEntry>,
}

#[derive(Deserialize)]
struct HostEntry {
    host: String,
    wss_port: u16,
}

/// Parse a `room_init` JSON body into [`RoomInfo`].
pub fn parse_room_init(body: &str) -> Result<RoomInfo> {
    let env: ApiEnvelope<RoomInitData> = serde_json::from_str(body)?;
    if env.code != 0 {
        return Err(DanmakuError::Api {
            code: env.code,
            message: env.message,
        });
    }
    let data = env.data.ok_or(DanmakuError::MissingField("data"))?;
    let status = match data.live_status {
        1 => LiveStatus::Live,
        2 => LiveStatus::Round,
        _ => LiveStatus::Offline,
    };
    Ok(RoomInfo {
        real_room_id: data.room_id,
        status,
    })
}

/// Parse a `getDanmuInfo` JSON body into [`DanmuInfo`].
pub fn parse_danmu_info(body: &str) -> Result<DanmuInfo> {
    let env: ApiEnvelope<DanmuInfoData> = serde_json::from_str(body)?;
    if env.code != 0 {
        return Err(DanmakuError::Api {
            code: env.code,
            message: env.message,
        });
    }
    let data = env.data.ok_or(DanmakuError::MissingField("data"))?;
    if data.host_list.is_empty() {
        return Err(DanmakuError::NoHost);
    }
    Ok(DanmuInfo {
        token: data.token,
        hosts: data
            .host_list
            .into_iter()
            .map(|h| DanmakuHost {
                host: h.host,
                wss_port: h.wss_port,
            })
            .collect(),
    })
}

/// A client that performs the HTTP handshake. Wraps a [`reqwest::Client`]
/// so callers can inject cookies (for higher rate limits / signed WBI).
pub struct BiliApi {
    http: reqwest::Client,
}

impl BiliApi {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http }
    }

    /// Build with a default client carrying a browser-like UA + referer.
    pub fn default_client() -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
            )
            .build()?;
        Ok(Self::new(http))
    }

    pub async fn room_init(&self, room_id: u64) -> Result<RoomInfo> {
        let body = self
            .http
            .get(ROOM_INIT)
            .query(&[("id", room_id)])
            .header("Referer", "https://live.bilibili.com/")
            .send()
            .await?
            .text()
            .await?;
        parse_room_init(&body)
    }

    pub async fn danmu_info(&self, room_id: u64) -> Result<DanmuInfo> {
        let body = self
            .http
            .get(DANMU_INFO)
            .query(&[("id", room_id), ("type", 0)])
            .header("Referer", "https://live.bilibili.com/")
            .send()
            .await?
            .text()
            .await?;
        parse_danmu_info(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_init_live() {
        let body = r#"{"code":0,"msg":"ok","data":{"room_id":6809855,"live_status":1}}"#;
        let info = parse_room_init(body).unwrap();
        assert_eq!(info.real_room_id, 6809855);
        assert_eq!(info.status, LiveStatus::Live);
    }

    #[test]
    fn room_init_offline_and_round() {
        let off = r#"{"code":0,"data":{"room_id":1,"live_status":0}}"#;
        assert_eq!(parse_room_init(off).unwrap().status, LiveStatus::Offline);
        let round = r#"{"code":0,"data":{"room_id":1,"live_status":2}}"#;
        assert_eq!(parse_room_init(round).unwrap().status, LiveStatus::Round);
    }

    #[test]
    fn room_init_error_code() {
        let body = r#"{"code":19002000,"message":"房间不存在","data":null}"#;
        let err = parse_room_init(body).unwrap_err();
        match err {
            DanmakuError::Api { code, .. } => assert_eq!(code, 19002000),
            _ => panic!("expected api error"),
        }
    }

    #[test]
    fn danmu_info_parses_hosts() {
        let body = r#"{"code":0,"data":{"token":"abc123","host_list":[
            {"host":"broadcastlv.chat.bilibili.com","wss_port":443},
            {"host":"tx-bj-live-comet-01.chat.bilibili.com","wss_port":2245}
        ]}}"#;
        let info = parse_danmu_info(body).unwrap();
        assert_eq!(info.token, "abc123");
        assert_eq!(info.hosts.len(), 2);
        assert_eq!(
            info.hosts[0].wss_url(),
            "wss://broadcastlv.chat.bilibili.com:443/sub"
        );
    }

    #[test]
    fn danmu_info_no_hosts_errors() {
        let body = r#"{"code":0,"data":{"token":"x","host_list":[]}}"#;
        assert!(matches!(
            parse_danmu_info(body).unwrap_err(),
            DanmakuError::NoHost
        ));
    }
}
