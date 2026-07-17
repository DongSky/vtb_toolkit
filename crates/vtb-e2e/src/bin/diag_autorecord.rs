//! Diagnostic: full AutoRecorder orchestration against a live room.
//! Feeds a real WentLive event → AutoRecorder resolves + records via
//! BiliResolver + FfmpegRecorder → WentOffline → verifies RecordingStopped
//! event carries valid metadata and a manifest was exported.

use std::time::Duration;

use vtb_recorder::auto::{AutoRecorder, AutoRecorderConfig, BiliResolver, RecorderEvent};
use vtb_recorder::ffmpeg::{FfmpegRecorder, SegmentPolicy};
use vtb_recorder::monitor::MonitorEvent;
use vtb_recorder::stream::{qn, StreamApi};

#[tokio::main]
async fn main() {
    let short: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(320);
    let secs: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(15);

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36")
        .build()
        .unwrap();
    // Resolve real room id first (AutoRecorder records by the id it's given).
    let real = StreamApi::new(client.clone()).room_status(short).await.unwrap().real_room_id;
    println!("real room {real}");

    let dir = std::path::PathBuf::from("/tmp/vtb-e2e-auto");
    let _ = std::fs::remove_dir_all(&dir);

    let config = AutoRecorderConfig {
        room_id: real,
        output_root: dir.clone(),
        segment: SegmentPolicy::Single,
        extension_override: None,
    };
    let resolver = BiliResolver::new(StreamApi::new(client), qn::ORIGINAL);
    let auto = AutoRecorder::new(config, resolver, FfmpegRecorder::default());

    let (mtx, mrx) = tokio::sync::mpsc::channel(8);
    let (etx, mut erx) = tokio::sync::mpsc::channel(8);
    let handle = tokio::spawn(auto.run(mrx, etx));

    // Simulate the monitor detecting the room going live.
    mtx.send(MonitorEvent::WentLive { room_id: real }).await.unwrap();

    // Wait for the started event.
    match erx.recv().await {
        Some(RecorderEvent::RecordingStarted { room_id, output_dir }) => {
            println!("✅ RecordingStarted room={room_id} dir={}", output_dir.display());
        }
        Some(RecorderEvent::RecordingError { message, .. }) => {
            eprintln!("❌ 录制启动失败: {message}");
            return;
        }
        other => {
            eprintln!("❌ 意外事件: {other:?}");
            return;
        }
    }

    println!("录制 {secs}s…");
    tokio::time::sleep(Duration::from_secs(secs)).await;

    // Simulate going offline → triggers graceful stop + manifest export.
    mtx.send(MonitorEvent::WentOffline { room_id: real }).await.unwrap();
    match erx.recv().await {
        Some(RecorderEvent::RecordingStopped { room_id, metadata }) => {
            println!("✅ RecordingStopped room={room_id}");
            println!("   分段数: {}", metadata.segments.len());
            println!("   总大小: {:.1} MB", metadata.total_bytes() as f64 / 1_048_576.0);
            println!("   时长: {:?}s", metadata.duration_secs());
            if metadata.total_bytes() == 0 {
                eprintln!("❌ 录制文件为空");
            }
        }
        other => eprintln!("❌ 意外停止事件: {other:?}"),
    }

    drop(mtx);
    let _ = handle.await;

    // Verify manifest on disk.
    let manifests: Vec<_> = walk(&dir).into_iter().filter(|p| p.to_string_lossy().ends_with(".meta.json")).collect();
    println!("\n=== 会话清单 ===");
    for m in &manifests {
        println!("  {}", m.display());
        if let Ok(txt) = std::fs::read_to_string(m) {
            let v: serde_json::Value = serde_json::from_str(&txt).unwrap();
            println!("  room_id={} segments={} ended_at={}",
                v["room_id"], v["segments"].as_array().map(|a| a.len()).unwrap_or(0), v["ended_at"]);
        }
    }
    if manifests.is_empty() {
        eprintln!("❌ 未导出会话清单");
    } else {
        println!("✅ AutoRecorder 完整编排验证通过");
    }
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() { out.extend(walk(&p)); } else { out.push(p); }
        }
    }
    out
}
