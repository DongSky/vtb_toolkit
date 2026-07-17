//! Diagnostic: exercise the REAL recorder path against a live room.
//! play_info → pick_best → FfmpegRecorder.spawn (with duration segmenting)
//! → verify multiple segment files are produced and playable.

use std::time::Duration;

use vtb_recorder::ffmpeg::{FfmpegRecorder, RecordOptions, SegmentPolicy};
use vtb_recorder::stream::{pick_best, qn, StreamApi};

#[tokio::main]
async fn main() {
    let short: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(320);
    let secs: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(20);

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36")
        .build()
        .unwrap();
    let api = StreamApi::new(client);

    // Resolve real room id via room_status.
    let status = api.room_status(short).await.expect("room_status");
    println!("room {short} → real {} status {:?}", status.real_room_id, status.status);
    let real = status.real_room_id;

    // Resolve streams — try highest, fall back down.
    let mut streams = Vec::new();
    for q in [qn::ORIGINAL, qn::BLUE_RAY, qn::HD, qn::FLUENT] {
        match api.play_info(real, q).await {
            Ok(s) if !s.is_empty() => {
                println!("play_info(qn={q}) → {} streams", s.len());
                streams = s;
                break;
            }
            Ok(_) => println!("play_info(qn={q}) → empty"),
            Err(e) => println!("play_info(qn={q}) err: {e}"),
        }
    }
    if streams.is_empty() {
        eprintln!("❌ 无可用流 (可能未开播)");
        return;
    }
    for s in &streams {
        println!("  流: {}/{} qn={} codec={} url={}...", s.protocol, s.format, s.qn, s.codec, &s.url.chars().take(60).collect::<String>());
    }
    let best = pick_best(&streams).unwrap();
    println!("选中: {}/{} qn={}", best.protocol, best.format, best.qn);

    // Record with duration segmenting (every ~8s → expect 2-3 segments in 20s).
    let dir = std::path::PathBuf::from("/tmp/vtb-e2e-rec");
    let _ = std::fs::remove_dir_all(&dir);
    let mut opts = RecordOptions::new(best.url.clone(), &dir, "seg");
    opts.extension = if best.format.eq_ignore_ascii_case("flv") { "flv" } else { "ts" }.into();
    opts.segment = SegmentPolicy::ByDuration { seconds: 8 };
    opts.headers = vec![("Referer".into(), "https://live.bilibili.com/".into())];
    opts.user_agent = Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36".into());

    let recorder = FfmpegRecorder::default();
    println!("ffmpeg: {}", recorder.probe().await.unwrap_or_else(|e| format!("PROBE FAILED: {e}")));

    let mut child = recorder.spawn(&opts).expect("spawn ffmpeg");
    println!("录制中 {secs}s (每8s切分)…");
    tokio::time::sleep(Duration::from_secs(secs)).await;

    // Graceful stop.
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        unsafe { libc::kill(pid as i32, libc::SIGINT); }
    }
    let _ = tokio::time::timeout(Duration::from_secs(8), child.wait()).await;
    let _ = child.start_kill();
    let _ = child.wait().await;

    // Verify segments.
    let outs = vtb_recorder::ffmpeg::list_outputs(&opts).unwrap_or_default();
    println!("\n=== 产出 {} 个分段 ===", outs.len());
    let mut total = 0u64;
    for p in &outs {
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        total += size;
        let dur = ffprobe_dur(p).await;
        println!("  {} ({:.1} KB, {}s)", p.file_name().unwrap().to_string_lossy(), size as f64 / 1024.0, dur);
    }
    println!("总大小 {:.1} MB", total as f64 / 1_048_576.0);
    if outs.len() >= 2 && total > 0 {
        println!("✅ 录制+切分验证通过");
    } else if outs.len() == 1 && total > 0 {
        println!("⚠️  只有1个分段（可能关键帧间隔>8s，正常）");
    } else {
        println!("❌ 录制失败");
    }
}

async fn ffprobe_dur(p: &std::path::Path) -> String {
    tokio::process::Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=nw=1:nk=1"])
        .arg(p)
        .output()
        .await
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}
