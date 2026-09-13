//! Recording health checks and post-processing.
//!
//! After a recording stops we verify it is actually usable — codec, real
//! picture (mean luma), audible audio — because a capture can "succeed"
//! while being black/silent (seen live with bilibili's HEVC-in-FLV).

use crate::error::{RecorderError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordingHealth {
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    /// Mean luma of sampled frames (0-255). Black video sits under ~17.
    pub mean_luma: Option<f64>,
    /// Overall RMS level in dBFS. Silence is ~-90 or -inf.
    pub audio_rms_db: Option<f64>,
    /// Overall verdict.
    pub ok: bool,
    /// Human-readable issues, empty when ok.
    pub issues: Vec<String>,
}

const BLACK_LUMA_THRESHOLD: f64 = 17.0;
const SILENCE_RMS_DB: f64 = -70.0;

/// Probe a recording and produce a health verdict.
pub async fn verify_recording(ffmpeg_dir_hint: &Path, file: &Path) -> RecordingHealth {
    let ffprobe = sibling_tool(ffmpeg_dir_hint, "ffprobe");
    let ffmpeg = ffmpeg_dir_hint.to_path_buf();

    let video_codec = probe_codec(&ffprobe, file, "v:0").await;
    let audio_codec = probe_codec(&ffprobe, file, "a:0").await;
    let mean_luma = probe_luma(&ffmpeg, file).await;
    let audio_rms_db = probe_rms(&ffmpeg, file).await;

    let mut issues = Vec::new();
    if video_codec.is_none() {
        issues.push("无视频流".to_string());
    }
    if let Some(l) = mean_luma {
        if l < BLACK_LUMA_THRESHOLD {
            issues.push(format!("疑似黑屏 (平均亮度 {l:.1})"));
        }
    }
    match audio_rms_db {
        None => issues.push("无音频流".to_string()),
        Some(db) if db < SILENCE_RMS_DB => issues.push(format!("疑似无声 (RMS {db:.1} dB)")),
        _ => {}
    }
    if let Some(codec) = &video_codec {
        if codec == "hevc" {
            issues.push("HEVC 编码，部分播放器不兼容".to_string());
        }
    }

    RecordingHealth {
        ok: issues.is_empty(),
        video_codec,
        audio_codec,
        mean_luma,
        audio_rms_db,
        issues,
    }
}

/// Remux a recording into an MP4 that standard players open directly.
/// Stream copy (lossless, fast); tags HEVC as hvc1 for Apple players.
pub async fn remux_mp4(ffmpeg: &Path, input: &Path) -> Result<PathBuf> {
    let output = input.with_extension("mp4");
    let ffprobe = sibling_tool(ffmpeg, "ffprobe");
    let is_hevc = probe_codec(&ffprobe, input, "v:0")
        .await
        .map(|c| c == "hevc")
        .unwrap_or(false);

    let mut cmd = tokio::process::Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(input)
        .args(["-c", "copy", "-movflags", "+faststart"]);
    if is_hevc {
        cmd.args(["-tag:v", "hvc1"]);
    }
    cmd.arg(&output);
    let status = run_tool(cmd)
        .await
        .ok_or_else(|| RecorderError::FfmpegFailed("remux failed or timed out".into()))?
        .status;
    if !status.success() {
        return Err(RecorderError::FfmpegFailed(format!("remux exit {status}")));
    }
    Ok(output)
}

/// Hard cap per probe/remux subprocess so a wedged tool can never hang a
/// session finalize.
const TOOL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Run a command to completion with output, bounded by [`TOOL_TIMEOUT`].
async fn run_tool(mut cmd: tokio::process::Command) -> Option<std::process::Output> {
    cmd.stdin(std::process::Stdio::null());
    cmd.kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let child = cmd.output();
    tokio::time::timeout(TOOL_TIMEOUT, child).await.ok()?.ok()
}

/// ffprobe usually sits next to ffmpeg; fall back to bare name (PATH).
fn sibling_tool(ffmpeg: &Path, tool: &str) -> PathBuf {
    match ffmpeg.parent() {
        Some(dir) if dir.as_os_str().len() > 0 => dir.join(tool),
        _ => PathBuf::from(tool),
    }
}

async fn probe_codec(ffprobe: &Path, file: &Path, stream: &str) -> Option<String> {
    let mut cmd = tokio::process::Command::new(ffprobe);
    cmd.args(["-v", "error", "-select_streams", stream])
        .args([
            "-show_entries",
            "stream=codec_name",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(file);
    let out = run_tool(cmd).await?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!name.is_empty()).then_some(name)
}

async fn probe_luma(ffmpeg: &Path, file: &Path) -> Option<f64> {
    let mut cmd = tokio::process::Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-i"]).arg(file).args([
        "-map",
        "0:v:0",
        "-vf",
        "signalstats,metadata=print:file=-",
        "-frames:v",
        "20",
        "-f",
        "null",
        "-",
    ]);
    let out = run_tool(cmd).await?;
    let text = String::from_utf8_lossy(&out.stdout);
    let lumas: Vec<f64> = text
        .lines()
        .filter_map(|l| l.split("signalstats.YAVG=").nth(1))
        .filter_map(|v| v.trim().parse().ok())
        .collect();
    (!lumas.is_empty()).then(|| lumas.iter().sum::<f64>() / lumas.len() as f64)
}

async fn probe_rms(ffmpeg: &Path, file: &Path) -> Option<f64> {
    let mut cmd = tokio::process::Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-i"]).arg(file).args([
        "-map",
        "0:a:0",
        "-af",
        "astats=metadata=1:reset=0,ametadata=print:file=-",
        "-f",
        "null",
        "-",
    ]);
    let out = run_tool(cmd).await?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .filter_map(|l| l.split("Overall.RMS_level=").nth(1))
        .filter_map(|v| v.trim().parse::<f64>().ok())
        .next_back()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ffmpeg_on_path() -> bool {
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).any(|d| d.join("ffmpeg").exists()))
            .unwrap_or(false)
    }

    async fn generate(dir: &Path, name: &str, video_args: &[&str]) -> PathBuf {
        let path = dir.join(name);
        let mut cmd = tokio::process::Command::new("ffmpeg");
        cmd.args(["-y", "-hide_banner", "-loglevel", "error"]);
        cmd.args(video_args);
        cmd.args([
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-c:a",
            "aac",
            "-shortest",
        ]);
        cmd.arg(&path);
        assert!(cmd.status().await.unwrap().success());
        path
    }

    #[tokio::test]
    async fn healthy_recording_passes() {
        if !ffmpeg_on_path() {
            eprintln!("skip: no ffmpeg");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let f = generate(
            dir.path(),
            "good.mp4",
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=320x240:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
            ],
        )
        .await;
        let h = verify_recording(Path::new("ffmpeg"), &f).await;
        assert!(h.ok, "issues: {:?}", h.issues);
        assert_eq!(h.video_codec.as_deref(), Some("h264"));
        assert!(h.mean_luma.unwrap() > 17.0);
        assert!(h.audio_rms_db.unwrap() > -70.0);
    }

    #[tokio::test]
    async fn black_silent_recording_flagged() {
        if !ffmpeg_on_path() {
            eprintln!("skip: no ffmpeg");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let f = generate(
            dir.path(),
            "black.mp4",
            &[
                "-f",
                "lavfi",
                "-i",
                "color=black:duration=2:size=320x240:rate=10",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=44100:cl=mono:d=2",
            ],
        )
        .await;
        let h = verify_recording(Path::new("ffmpeg"), &f).await;
        assert!(!h.ok);
        let joined = h.issues.join(";");
        assert!(joined.contains("黑屏"), "issues: {joined}");
        assert!(joined.contains("无声"), "issues: {joined}");
    }

    #[tokio::test]
    async fn remux_produces_playable_mp4() {
        if !ffmpeg_on_path() {
            eprintln!("skip: no ffmpeg");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        // Make an FLV first.
        let flv = dir.path().join("in.flv");
        let st = tokio::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=320x240:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-c:a",
                "aac",
                "-shortest",
                "-f",
                "flv",
            ])
            .arg(&flv)
            .status()
            .await
            .unwrap();
        assert!(st.success());

        let mp4 = remux_mp4(Path::new("ffmpeg"), &flv).await.unwrap();
        assert!(mp4.exists());
        assert_eq!(mp4.extension().unwrap(), "mp4");
        let h = verify_recording(Path::new("ffmpeg"), &mp4).await;
        assert!(h.ok, "remuxed file unhealthy: {:?}", h.issues);
    }
}
