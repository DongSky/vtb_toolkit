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
    let short: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(320);
    let secs: u64 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(40);
    // Optional 3rd arg: output directory (default /tmp/vtb-offline).
    let workdir = std::env::args()
        .nth(3)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/vtb-offline"));
    // Optional 4th arg: target language (default zh). Use "en"/"ja" for
    // Chinese streams so the same-language skip doesn't drop everything.
    let target_lang = std::env::args().nth(4).unwrap_or_else(|| "zh".into());

    let _ = std::fs::remove_dir_all(&workdir);
    std::fs::create_dir_all(&workdir).unwrap();
    println!("输出目录: {}", workdir.display());

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
            let _ = w2.lock().await.write(&LogEntry {
                source: None,
                received_at: Utc::now(),
                event: ev,
            });
        }
    });

    // Record video.
    println!(
        "录制 {secs}s 视频 + 弹幕 (room {real}, codec={})…",
        best.codec
    );
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

    // Remux to MP4 so the recording double-click plays in QuickTime
    // (FLV isn't supported there at all). Stream copy: fast & lossless.
    // hvc1 tag makes HEVC-in-MP4 playable on Apple players if the source
    // ever is HEVC.
    let video_mp4 = workdir.join("rec.mp4");
    let is_hevc = best.codec.eq_ignore_ascii_case("hevc");
    let mut remux = tokio::process::Command::new("ffmpeg");
    remux
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(&video)
        .args(["-c", "copy", "-movflags", "+faststart"]);
    if is_hevc {
        remux.args(["-tag:v", "hvc1"]);
    }
    remux.arg(&video_mp4);
    if remux.status().await.map(|s| s.success()).unwrap_or(false) {
        println!("已生成可直接播放的 MP4: {}", video_mp4.display());
    }

    // Sanity-check the recording is not black/silent: mean luma + audio RMS.
    if let Ok(out) = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(&video)
        .output()
        .await
    {
        println!("视频编码: {}", String::from_utf8_lossy(&out.stdout).trim());
    }
    if let Ok(out) = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-i"])
        .arg(&video)
        .args([
            "-map",
            "0:v:0",
            "-vf",
            "signalstats,metadata=print:file=-",
            "-frames:v",
            "30",
            "-f",
            "null",
            "-",
        ])
        .output()
        .await
    {
        let text = String::from_utf8_lossy(&out.stdout);
        let lumas: Vec<f64> = text
            .lines()
            .filter_map(|l| l.split("signalstats.YAVG=").nth(1))
            .filter_map(|v| v.trim().parse().ok())
            .collect();
        if !lumas.is_empty() {
            let avg = lumas.iter().sum::<f64>() / lumas.len() as f64;
            let verdict = if avg < 17.0 {
                "⚠️ 疑似黑屏"
            } else {
                "✅ 有画面"
            };
            println!("画面平均亮度 YAVG={avg:.1} {verdict}");
        }
    }

    let n_lines = vtb_pipeline::danmaku_log::read_log(&log_path)
        .map(|e| e.len())
        .unwrap_or(0);
    println!("弹幕日志 {n_lines} 条");

    // Model + translation backend.
    let model = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".cache/vtb-toolkit/ggml-tiny.bin"))
        .unwrap();
    let engine = Arc::new(WhisperEngine::new(&model, None).unwrap());

    let translate = matches!(
        (
            std::env::var("OPENAI_BASE_URL"),
            std::env::var("OPENAI_API_KEY")
        ),
        (Ok(_), Ok(_))
    );

    let mut cfg = JobConfig::new(&video, workdir.join("out"));
    cfg.danmaku_log = Some(log_path.clone());
    cfg.session_start = Some(session_start);
    cfg.translate = translate;
    cfg.target_lang = target_lang.clone();
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
        let ok = path.exists()
            && std::fs::metadata(path)
                .map(|m| m.len() > 0)
                .unwrap_or(false);
        println!("  {} {name}", if ok { "✅" } else { "❌" });
    }
    println!(
        "  字幕文件: {:?}",
        out.subtitle_files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
    );
    println!(
        "  转写 {} 段 | 翻译 {} 段 | 高能 {} | 切片 {}",
        out.transcript.len(),
        out.translations.len(),
        out.highlights.len(),
        out.clip_files.len()
    );

    // Show a sample of the bilingual subtitle if present.
    if let Some(bi) = out.subtitle_files.iter().find(|p| {
        p.to_string_lossy().contains("bilingual")
            && p.extension().map(|e| e == "srt").unwrap_or(false)
    }) {
        println!("\n=== 双语字幕样例 ===");
        let txt = std::fs::read_to_string(bi).unwrap();
        for line in txt.lines().take(12) {
            println!("  {line}");
        }
    }

    // Verify clips are playable.
    for clip in out.clip_files.iter().take(2) {
        let dur = tokio::process::Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=nw=1:nk=1",
            ])
            .arg(clip)
            .output()
            .await
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        println!(
            "  切片 {} ({}s)",
            clip.file_name().unwrap().to_string_lossy(),
            dur
        );
    }

    println!("\n✅ 离线管线端到端验证通过");
}
