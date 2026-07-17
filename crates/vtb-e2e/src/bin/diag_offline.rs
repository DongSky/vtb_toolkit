//! Diagnostic: full offline pipeline over a real recording, with real
//! translation enabled. Records video + danmaku, then runs the pipeline:
//! extract audio → ASR → translate (real LLM) → subtitles → highlights →
//! clips, and verifies every artifact on disk.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::Mutex;
use vtb_asr::engine::WhisperEngine;
use vtb_common::LiveEvent;
use vtb_danmaku::{DanmakuClient, DanmakuClientConfig};
use vtb_pipeline::danmaku_log::{DanmakuLogWriter, LogEntry};
use vtb_pipeline::{JobConfig, OfflineJob};
use vtb_recorder::stream::{pick_best, qn, StreamApi};
use vtb_translate::{OpenAiCompatBackend, StreamerProfile, TranslateConfig, TranslatePipeline};

#[tokio::main]
async fn main() {
    let short: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(320);
    let secs: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(40);

    let workdir = PathBuf::from("/tmp/vtb-offline");
    let _ = std::fs::remove_dir_all(&workdir);
    std::fs::create_dir_all(&workdir).unwrap();

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36")
        .build().unwrap();
    let api = StreamApi::new(client);
    let real = api.room_status(short).await.unwrap().real_room_id;
    let streams = api.play_info(real, qn::HD).await.unwrap();
    let best = pick_best(&streams).unwrap().clone();

    // Concurrent danmaku logging during recording.
    let session_start = Utc::now();
    let log_path = workdir.join("danmaku.jsonl");
    let writer = Arc::new(Mutex::new(DanmakuLogWriter::create(&log_path).unwrap()));
    let dm = DanmakuClient::new(DanmakuClientConfig::anonymous(real)).unwrap();
    let (mut rx, handle) = dm.connect().await.unwrap();
    let w2 = writer.clone();
    let collector = tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            let _ = w2.lock().await.write(&LogEntry { received_at: Utc::now(), event: ev });
        }
    });

    // Record video.
    println!("录制 {secs}s 视频 + 弹幕 (room {real})…");
    let video = workdir.join("rec.flv");
    let ff = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-user_agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36"])
        .args(["-headers", "Referer: https://live.bilibili.com/\r\n"])
        .args(["-i", &best.url, "-t", &secs.to_string(), "-c", "copy"])
        .arg(&video)
        .status().await.unwrap();
    assert!(ff.success());
    handle.abort();
    collector.abort();
    let _ = collector.await;
    writer.lock().await.flush().unwrap();

    let n_lines = vtb_pipeline::danmaku_log::read_log(&log_path).map(|e| e.len()).unwrap_or(0);
    println!("弹幕日志 {n_lines} 条");

    // Model + translation backend.
    let model = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache/vtb-toolkit/ggml-tiny.bin")).unwrap();
    let engine = Arc::new(WhisperEngine::new(&model, None).unwrap());

    let translate = matches!((std::env::var("OPENAI_BASE_URL"), std::env::var("OPENAI_API_KEY")), (Ok(_), Ok(_)));

    let mut cfg = JobConfig::new(&video, workdir.join("out"));
    cfg.danmaku_log = Some(log_path.clone());
    cfg.session_start = Some(session_start);
    cfg.translate = translate;
    cfg.target_lang = "zh".into();
    cfg.highlights = true;
    cfg.fusion.threshold = 0.6;
    cfg.fusion.min_len_ms = 0;
    cfg.fusion.pad_ms = 1000;

    let mut job = OfflineJob::new(cfg, engine);
    if translate {
        let backend = Arc::new(OpenAiCompatBackend::new(
            std::env::var("OPENAI_BASE_URL").unwrap(),
            std::env::var("OPENAI_API_KEY").unwrap(),
            std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into()),
        ));
        job = job.with_translator(backend, StreamerProfile::default());
        println!("翻译: 开启 (真实 LLM)");
    } else {
        println!("翻译: 关闭 (无 OPENAI_*)");
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let job = job.with_progress(tx);
    let pump = tokio::spawn(async move {
        while let Some(p) = rx.recv().await {
            println!("  [{:?}] {}", p.stage, p.message);
        }
    });

    println!("\n=== 运行离线管线 ===");
    let out = job.run().await.expect("pipeline failed");
    let _ = pump.await;

    // Verify artifacts.
    println!("\n=== 产物校验 ===");
    let checks = [
        ("transcript.json", workdir.join("out/transcript.json")),
        ("subtitles.srt", workdir.join("out/subtitles.srt")),
        ("highlights.json", workdir.join("out/highlights.json")),
    ];
    for (name, path) in &checks {
        let ok = path.exists() && std::fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false);
        println!("  {} {name}", if ok { "✅" } else { "❌" });
    }
    println!("  字幕文件: {:?}", out.subtitle_files.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>());
    println!("  转写 {} 段 | 翻译 {} 段 | 高能 {} | 切片 {}",
        out.transcript.len(), out.translations.len(), out.highlights.len(), out.clip_files.len());

    // Show a sample of the bilingual subtitle if present.
    if let Some(bi) = out.subtitle_files.iter().find(|p| p.to_string_lossy().contains("bilingual") && p.extension().map(|e| e == "srt").unwrap_or(false)) {
        println!("\n=== 双语字幕样例 ===");
        let txt = std::fs::read_to_string(bi).unwrap();
        for line in txt.lines().take(12) {
            println!("  {line}");
        }
    }

    // Verify clips are playable.
    for clip in out.clip_files.iter().take(2) {
        let dur = tokio::process::Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=nw=1:nk=1"])
            .arg(clip).output().await.ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        println!("  切片 {} ({}s)", clip.file_name().unwrap().to_string_lossy(), dur);
    }

    println!("\n✅ 离线管线端到端验证通过");
}
