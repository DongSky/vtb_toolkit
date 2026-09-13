//! Opt-in live smoke: VTB_YOUTUBE_URL points to a currently public live stream.
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use vtb_recorder::{
    ffmpeg::{FfmpegRecorder, RecordOptions},
    platform::{Platform, YtDlpPlatform},
};

#[tokio::test]
#[ignore = "requires a live YouTube URL, yt-dlp, FFmpeg and network"]
async fn records_video_and_audio_and_stops_gracefully() {
    let url = std::env::var("VTB_YOUTUBE_URL").expect("set VTB_YOUTUBE_URL");
    let platform = YtDlpPlatform {
        binary: std::env::var("VTB_YTDLP")
            .unwrap_or_else(|_| "yt-dlp".into())
            .into(),
    };
    let stream = platform.resolve(&url).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut options = RecordOptions::new(stream.url, dir.path(), "live-smoke");
    options.audio_input = stream.audio;
    options.headers = stream.headers;
    options.extension = "mkv".into();
    let mut child = FfmpegRecorder::default().spawn(&options).unwrap();
    let seconds = std::env::var("VTB_RECORD_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(25);
    tokio::time::sleep(Duration::from_secs(seconds)).await;
    // Keep the pipe open until FFmpeg reads q (Windows PeekNamedPipe needs it).
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"q\n").await.unwrap();
    let status = tokio::time::timeout(Duration::from_secs(20), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(status.success(), "FFmpeg failed");
    let output = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "json",
        ])
        .arg(options.output_template())
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let streams = info["streams"].as_array().unwrap();
    assert!(streams.iter().any(|s| s["codec_type"] == "video"));
    assert!(streams.iter().any(|s| s["codec_type"] == "audio"));
}
