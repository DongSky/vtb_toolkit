//! End-to-end live test harness for the VTB toolkit.
//!
//! Exercises every module against a real Bilibili live room:
//!   1. danmaku streaming (real wss) + JSONL logging
//!   2. open-status detection + playurl resolution + ffmpeg recording
//!   3. ASR transcription of the recorded audio (whisper)
//!   4. highlight detection (danmaku + audio fusion) + clipping
//!   5. translation pipeline (mock backend — no API key needed)
//!   6. full offline pipeline over the recording
//!
//! Usage: `cargo run -p vtb-e2e -- <short_room_id> [record_secs]`

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use vtb_common::LiveEvent;

mod steps;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let room_id: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(320);
    let record_secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(30);

    let workdir = PathBuf::from("/tmp/vtb-e2e");
    let _ = std::fs::remove_dir_all(&workdir);
    std::fs::create_dir_all(&workdir).unwrap();

    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║  VTB Toolkit 端到端测试                                    ║");
    println!("║  房间: {room_id}  录制时长: {record_secs}s  工作目录: {}", workdir.display());
    println!("╚══════════════════════════════════════════════════════════╝\n");

    // ---- Stage 0: resolve room ----
    let (real_room, status) = steps::resolve_room(room_id).await;

    // ---- Stage 1: danmaku streaming + logging ----
    let (events, log_path) =
        steps::danmaku_stage(real_room, &workdir, Duration::from_secs(record_secs)).await;

    // ---- Stage 2: recording (only if live) ----
    let session_start = Utc::now() - chrono::Duration::seconds(record_secs as i64);
    let recording = if status == vtb_common::LiveStatus::Live {
        steps::record_stage(real_room, &workdir, record_secs).await
    } else {
        println!("⚠️  房间未开播，跳过录制/ASR/切片/离线阶段\n");
        None
    };

    // ---- Stage 5: translation pipeline (mock) — independent of live ----
    steps::translate_stage().await;

    if let Some(recording) = recording {
        // ---- Stage 3: ASR ----
        let model = steps::ensure_model().await;
        let transcript = steps::asr_stage(&recording, model.as_deref()).await;

        // ---- Stage 4: highlight + clip ----
        steps::highlight_stage(&recording, &events, session_start, &workdir).await;

        // ---- Stage 6: full offline pipeline ----
        if let Some(model) = &model {
            steps::offline_stage(&recording, &log_path, session_start, model, &workdir)
                .await;
        }

        let _ = transcript;
    }

    steps::summary();
}

/// Count events by variant for reporting.
pub fn tally(events: &[LiveEvent]) -> std::collections::BTreeMap<&'static str, usize> {
    let mut m = std::collections::BTreeMap::new();
    for ev in events {
        let key = match ev {
            LiveEvent::Danmaku(_) => "弹幕",
            LiveEvent::SuperChat(_) => "SC",
            LiveEvent::Gift(_) => "礼物",
            LiveEvent::GuardBuy(_) => "上舰",
            LiveEvent::Enter(_) => "进场",
            LiveEvent::Like(_) => "点赞",
            LiveEvent::WatchedChange { .. } => "看过人数",
            LiveEvent::LiveStart { .. } => "开播",
            LiveEvent::LiveEnd { .. } => "下播",
        };
        *m.entry(key).or_insert(0) += 1;
    }
    m
}

pub type Shared<T> = Arc<T>;
