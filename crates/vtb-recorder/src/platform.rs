//! Multi-platform abstraction: bilibili natively, YouTube/Twitch and
//! hundreds more via yt-dlp when installed.

use crate::auto::{ResolvedStream, StreamResolver};
use crate::error::{RecorderError, Result};
use async_trait::async_trait;
use vtb_common::LiveStatus;

/// A live-stream platform: status checks + stream resolution.
#[async_trait]
pub trait Platform: Send + Sync {
    fn name(&self) -> &str;
    /// `id` is platform-specific: room id for bilibili, URL for yt-dlp.
    async fn check_live(&self, id: &str) -> Result<LiveStatus>;
    async fn resolve(&self, id: &str) -> Result<ResolvedStream>;
}

/// Bilibili platform backed by the native implementation.
pub struct BilibiliPlatform {
    api: crate::stream::StreamApi,
    resolver: crate::auto::BiliResolver,
}

impl BilibiliPlatform {
    pub fn new(client: reqwest::Client, qn: u32) -> Self {
        Self {
            api: crate::stream::StreamApi::new(client.clone()),
            resolver: crate::auto::BiliResolver::new(
                crate::stream::StreamApi::new(client),
                qn,
            ),
        }
    }

    fn parse_id(id: &str) -> Result<u64> {
        id.trim()
            .trim_start_matches("https://live.bilibili.com/")
            .split(['/', '?'])
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| RecorderError::Config(format!("无效的B站房间号: {id}")))
    }
}

#[async_trait]
impl Platform for BilibiliPlatform {
    fn name(&self) -> &str {
        "bilibili"
    }

    async fn check_live(&self, id: &str) -> Result<LiveStatus> {
        let room = Self::parse_id(id)?;
        Ok(self.api.room_status(room).await?.status)
    }

    async fn resolve(&self, id: &str) -> Result<ResolvedStream> {
        let room = Self::parse_id(id)?;
        self.resolver.resolve(room).await
    }
}

/// Generic platform driven by yt-dlp (YouTube / Twitch / …). `id` is the
/// channel or watch URL.
pub struct YtDlpPlatform {
    pub binary: std::path::PathBuf,
}

impl Default for YtDlpPlatform {
    fn default() -> Self {
        Self {
            binary: "yt-dlp".into(),
        }
    }
}

impl YtDlpPlatform {
    pub fn available(&self) -> bool {
        which_exists(&self.binary)
    }

    /// Args to query live status: prints `True`/`False`/`None`.
    pub fn status_args(url: &str) -> Vec<String> {
        vec![
            "--no-warnings".into(),
            "--print".into(),
            "%(is_live)s".into(),
            "--playlist-items".into(),
            "1".into(),
            url.to_string(),
        ]
    }

    /// Args to resolve the direct stream URL of the best av format.
    pub fn resolve_args(url: &str) -> Vec<String> {
        vec![
            "--no-warnings".into(),
            "-g".into(),
            "-f".into(),
            "best".into(),
            url.to_string(),
        ]
    }
}

fn which_exists(binary: &std::path::Path) -> bool {
    if binary.components().count() > 1 {
        return binary.exists();
    }
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(binary).exists()))
        .unwrap_or(false)
}

/// Parse yt-dlp `--print %(is_live)s` output.
pub fn parse_is_live(stdout: &str) -> LiveStatus {
    match stdout.trim() {
        "True" => LiveStatus::Live,
        _ => LiveStatus::Offline,
    }
}

#[async_trait]
impl Platform for YtDlpPlatform {
    fn name(&self) -> &str {
        "yt-dlp"
    }

    async fn check_live(&self, id: &str) -> Result<LiveStatus> {
        if !self.available() {
            return Err(RecorderError::Config(
                "yt-dlp 未安装（brew install yt-dlp 以支持 YouTube/Twitch）".into(),
            ));
        }
        let out = tokio::process::Command::new(&self.binary)
            .args(Self::status_args(id))
            .output()
            .await
            .map_err(|e| RecorderError::Config(format!("yt-dlp: {e}")))?;
        Ok(parse_is_live(&String::from_utf8_lossy(&out.stdout)))
    }

    async fn resolve(&self, id: &str) -> Result<ResolvedStream> {
        if !self.available() {
            return Err(RecorderError::Config("yt-dlp 未安装".into()));
        }
        let out = tokio::process::Command::new(&self.binary)
            .args(Self::resolve_args(id))
            .output()
            .await
            .map_err(|e| RecorderError::Config(format!("yt-dlp: {e}")))?;
        if !out.status.success() {
            return Err(RecorderError::Config(format!(
                "yt-dlp 取流失败: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        let url = String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if url.is_empty() {
            return Err(RecorderError::NoStreamUrl);
        }
        // HLS (m3u8) is typical for both YouTube and Twitch lives.
        let extension = if url.contains(".m3u8") { "ts" } else { "mp4" };
        Ok(ResolvedStream {
            url,
            headers: vec![],
            user_agent: None,
            extension: extension.into(),
            backup_urls: vec![],
        })
    }
}

/// Route an id/URL to a platform implementation.
pub fn platform_for(id: &str, client: reqwest::Client, qn: u32) -> Box<dyn Platform> {
    let lower = id.trim().to_lowercase();
    if lower.contains("youtube.com")
        || lower.contains("youtu.be")
        || lower.contains("twitch.tv")
        || lower.starts_with("http") && !lower.contains("bilibili.com")
    {
        Box::new(YtDlpPlatform::default())
    } else {
        Box::new(BilibiliPlatform::new(client, qn))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bilibili_id_parsing() {
        assert_eq!(BilibiliPlatform::parse_id("24158116").unwrap(), 24158116);
        assert_eq!(
            BilibiliPlatform::parse_id("https://live.bilibili.com/320").unwrap(),
            320
        );
        assert_eq!(
            BilibiliPlatform::parse_id("https://live.bilibili.com/320?spm=x").unwrap(),
            320
        );
        assert!(BilibiliPlatform::parse_id("not a room").is_err());
    }

    #[test]
    fn ytdlp_arg_builders() {
        let s = YtDlpPlatform::status_args("https://twitch.tv/foo");
        assert!(s.contains(&"%(is_live)s".to_string()));
        assert_eq!(s.last().unwrap(), "https://twitch.tv/foo");
        let r = YtDlpPlatform::resolve_args("https://youtube.com/watch?v=x");
        assert!(r.contains(&"-g".to_string()));
        assert_eq!(r.last().unwrap(), "https://youtube.com/watch?v=x");
    }

    #[test]
    fn is_live_parsing() {
        assert_eq!(parse_is_live("True\n"), LiveStatus::Live);
        assert_eq!(parse_is_live("False"), LiveStatus::Offline);
        assert_eq!(parse_is_live("None"), LiveStatus::Offline);
        assert_eq!(parse_is_live(""), LiveStatus::Offline);
    }

    #[test]
    fn platform_routing() {
        let client = reqwest::Client::new();
        assert_eq!(
            platform_for("24158116", client.clone(), 10000).name(),
            "bilibili"
        );
        assert_eq!(
            platform_for("https://live.bilibili.com/320", client.clone(), 10000).name(),
            "bilibili"
        );
        assert_eq!(
            platform_for("https://www.youtube.com/watch?v=abc", client.clone(), 10000).name(),
            "yt-dlp"
        );
        assert_eq!(
            platform_for("https://twitch.tv/somebody", client, 10000).name(),
            "yt-dlp"
        );
    }

    #[tokio::test]
    async fn ytdlp_graceful_when_missing() {
        let p = YtDlpPlatform {
            binary: "/definitely-not-installed-ytdlp".into(),
        };
        let err = p.check_live("https://twitch.tv/x").await.unwrap_err();
        assert!(err.to_string().contains("yt-dlp"));
    }
}
