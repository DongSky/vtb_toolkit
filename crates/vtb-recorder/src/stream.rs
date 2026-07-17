//! Bilibili live stream URL resolution (`getRoomPlayInfo`) and open-status
//! queries (`room_init`). Parsing is separated from HTTP for testability.

use crate::error::{RecorderError, Result};
use serde::Deserialize;
use vtb_common::LiveStatus;

const ROOM_INIT: &str = "https://api.live.bilibili.com/room/v1/Room/room_init";
const PLAY_INFO: &str =
    "https://api.live.bilibili.com/xlive/web-room/v2/index/getRoomPlayInfo";

/// Quality codes accepted by `qn`.
pub mod qn {
    pub const FLUENT: u32 = 80;
    pub const HD: u32 = 150;
    pub const SUPER_HD: u32 = 250;
    pub const BLUE_RAY: u32 = 400;
    pub const ORIGINAL: u32 = 10000;
    pub const UHD_4K: u32 = 20000;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomStatus {
    pub real_room_id: u64,
    pub uid: u64,
    pub status: LiveStatus,
}

#[derive(Deserialize)]
struct Envelope<T> {
    code: i64,
    #[serde(default)]
    message: String,
    data: Option<T>,
}

#[derive(Deserialize)]
struct RoomInitData {
    room_id: u64,
    #[serde(default)]
    uid: u64,
    live_status: u8,
}

pub fn parse_room_status(body: &str) -> Result<RoomStatus> {
    let env: Envelope<RoomInitData> =
        serde_json::from_str(body).map_err(RecorderError::Json)?;
    if env.code != 0 {
        return Err(RecorderError::Config(format!(
            "room_init code {}: {}",
            env.code, env.message
        )));
    }
    let data = env
        .data
        .ok_or_else(|| RecorderError::Config("room_init missing data".into()))?;
    let status = match data.live_status {
        1 => LiveStatus::Live,
        2 => LiveStatus::Round,
        _ => LiveStatus::Offline,
    };
    Ok(RoomStatus {
        real_room_id: data.room_id,
        uid: data.uid,
        status,
    })
}

// --- playurl parsing ---

#[derive(Deserialize)]
struct PlayInfoData {
    playurl_info: Option<PlayurlInfo>,
}

#[derive(Deserialize)]
struct PlayurlInfo {
    playurl: Playurl,
}

#[derive(Deserialize)]
struct Playurl {
    stream: Vec<Stream>,
}

#[derive(Deserialize)]
struct Stream {
    #[serde(default)]
    protocol_name: String,
    format: Vec<Format>,
}

#[derive(Deserialize)]
struct Format {
    #[serde(default)]
    format_name: String,
    codec: Vec<Codec>,
}

#[derive(Deserialize)]
struct Codec {
    #[serde(default)]
    codec_name: String,
    base_url: String,
    url_info: Vec<UrlInfo>,
    #[serde(default)]
    current_qn: u32,
    #[serde(default)]
    accept_qn: Vec<u32>,
}

#[derive(Deserialize)]
struct UrlInfo {
    host: String,
    extra: String,
}

/// A resolved playable stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayStream {
    pub url: String,
    pub protocol: String,
    pub format: String,
    pub codec: String,
    pub qn: u32,
    pub accept_qn: Vec<u32>,
}

/// Parse `getRoomPlayInfo` and return every resolvable stream URL.
pub fn parse_play_info(body: &str) -> Result<Vec<PlayStream>> {
    let env: Envelope<PlayInfoData> =
        serde_json::from_str(body).map_err(RecorderError::Json)?;
    if env.code != 0 {
        return Err(RecorderError::Config(format!(
            "getRoomPlayInfo code {}: {}",
            env.code, env.message
        )));
    }
    let data = env.data.ok_or(RecorderError::NoStreamUrl)?;
    let info = data.playurl_info.ok_or(RecorderError::NoStreamUrl)?;

    let mut out = Vec::new();
    for stream in &info.playurl.stream {
        for format in &stream.format {
            for codec in &format.codec {
                if let Some(u) = codec.url_info.first() {
                    out.push(PlayStream {
                        url: format!("{}{}{}", u.host, codec.base_url, u.extra),
                        protocol: stream.protocol_name.clone(),
                        format: format.format_name.clone(),
                        codec: codec.codec_name.clone(),
                        qn: codec.current_qn,
                        accept_qn: codec.accept_qn.clone(),
                    });
                }
            }
        }
    }
    if out.is_empty() {
        return Err(RecorderError::NoStreamUrl);
    }
    Ok(out)
}

/// Pick the best stream: prefer FLV over HLS (simpler to record), then AVC
/// over HEVC (bilibili's HEVC-in-FLV is a private extension that QuickTime
/// and many players render as black/silent), then the highest quality.
pub fn pick_best(streams: &[PlayStream]) -> Option<&PlayStream> {
    streams.iter().max_by_key(|s| {
        (
            s.format.eq_ignore_ascii_case("flv") as u32,
            s.codec.eq_ignore_ascii_case("avc") as u32,
            s.qn,
        )
    })
}

/// HTTP client for stream queries.
pub struct StreamApi {
    http: reqwest::Client,
}

impl StreamApi {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http }
    }

    pub async fn room_status(&self, room_id: u64) -> Result<RoomStatus> {
        let body = self
            .http
            .get(ROOM_INIT)
            .query(&[("id", room_id)])
            .header("Referer", "https://live.bilibili.com/")
            .send()
            .await?
            .text()
            .await?;
        parse_room_status(&body)
    }

    pub async fn play_info(&self, room_id: u64, qn: u32) -> Result<Vec<PlayStream>> {
        let body = self
            .http
            .get(PLAY_INFO)
            .query(&[
                ("room_id", room_id.to_string()),
                ("protocol", "0,1".into()),
                ("format", "0,1,2".into()),
                ("codec", "0,1".into()),
                ("qn", qn.to_string()),
                ("platform", "web".into()),
                ("ptype", "8".into()),
            ])
            .header("Referer", "https://live.bilibili.com/")
            .send()
            .await?
            .text()
            .await?;
        parse_play_info(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_status_live() {
        let body = r#"{"code":0,"data":{"room_id":123,"uid":9,"live_status":1}}"#;
        let s = parse_room_status(body).unwrap();
        assert_eq!(s.real_room_id, 123);
        assert_eq!(s.uid, 9);
        assert_eq!(s.status, LiveStatus::Live);
    }

    #[test]
    fn room_status_error() {
        let body = r#"{"code":-400,"message":"bad","data":null}"#;
        assert!(parse_room_status(body).is_err());
    }

    #[test]
    fn play_info_builds_full_url() {
        let body = r#"{"code":0,"data":{"playurl_info":{"playurl":{"stream":[
            {"protocol_name":"http_stream","format":[
                {"format_name":"flv","codec":[
                    {"codec_name":"avc","base_url":"/live/123.flv?x=1",
                     "current_qn":10000,"accept_qn":[10000,400,250],
                     "url_info":[{"host":"https://cn-live.bilivideo.com","extra":"&t=abc"}]}
                ]}
            ]}
        ]}}}}"#;
        let streams = parse_play_info(body).unwrap();
        assert_eq!(streams.len(), 1);
        assert_eq!(
            streams[0].url,
            "https://cn-live.bilivideo.com/live/123.flv?x=1&t=abc"
        );
        assert_eq!(streams[0].qn, 10000);
        assert_eq!(streams[0].format, "flv");
    }

    #[test]
    fn play_info_no_stream_errors() {
        let body = r#"{"code":0,"data":{"playurl_info":{"playurl":{"stream":[]}}}}"#;
        assert!(matches!(
            parse_play_info(body).unwrap_err(),
            RecorderError::NoStreamUrl
        ));
    }

    #[test]
    fn pick_best_prefers_flv_and_high_qn() {
        let streams = vec![
            PlayStream {
                url: "hls-low".into(),
                protocol: "http_hls".into(),
                format: "fmp4".into(),
                codec: "avc".into(),
                qn: 10000,
                accept_qn: vec![],
            },
            PlayStream {
                url: "flv-mid".into(),
                protocol: "http_stream".into(),
                format: "flv".into(),
                codec: "avc".into(),
                qn: 400,
                accept_qn: vec![],
            },
            PlayStream {
                url: "flv-high".into(),
                protocol: "http_stream".into(),
                format: "flv".into(),
                codec: "avc".into(),
                qn: 10000,
                accept_qn: vec![],
            },
        ];
        assert_eq!(pick_best(&streams).unwrap().url, "flv-high");
    }

    /// Regression (found live): bilibili returns avc and hevc FLV variants
    /// at the same qn, hevc listed after avc. `max_by_key` keeps the LAST
    /// max, so we recorded HEVC-in-FLV — which players render as a black,
    /// silent video. AVC must win at equal qn/format.
    #[test]
    fn pick_best_prefers_avc_over_hevc_at_same_qn() {
        let mk = |codec: &str, url: &str| PlayStream {
            url: url.into(),
            protocol: "http_stream".into(),
            format: "flv".into(),
            codec: codec.into(),
            qn: 250,
            accept_qn: vec![],
        };
        // hevc after avc, same qn — the exact live layout that broke.
        let streams = vec![mk("avc", "flv-avc"), mk("hevc", "flv-hevc")];
        assert_eq!(pick_best(&streams).unwrap().url, "flv-avc");
        // Order-independent.
        let streams = vec![mk("hevc", "flv-hevc"), mk("avc", "flv-avc")];
        assert_eq!(pick_best(&streams).unwrap().url, "flv-avc");
    }
}
