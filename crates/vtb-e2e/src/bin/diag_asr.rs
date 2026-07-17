//! Diagnostic: ASR over fresh live audio. Records N seconds, extracts PCM,
//! runs VAD to show utterance boundaries, then whisper-transcribes each.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use vtb_asr::engine::WhisperEngine;
use vtb_asr::streaming::{transcribe_buffer, StreamingConfig};
use vtb_asr::vad::{detect_spans, VadConfig};
use vtb_recorder::stream::{pick_best, qn, StreamApi};

#[tokio::main]
async fn main() {
    let short: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(320);
    let secs: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(40);

    // 1. Record fresh audio directly (audio-only, faster than full record).
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36")
        .build().unwrap();
    let api = StreamApi::new(client);
    let real = api.room_status(short).await.unwrap().real_room_id;
    let streams = api.play_info(real, qn::HD).await.unwrap();
    let best = pick_best(&streams).unwrap();
    println!("录制 {secs}s 音频 (room {real})…");

    let wav = "/tmp/vtb-asr-test.wav";
    let status = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-user_agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36"])
        .args(["-headers", "Referer: https://live.bilibili.com/\r\n"])
        .args(["-i", &best.url, "-t", &secs.to_string()])
        .args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", wav])
        .status().await.unwrap();
    assert!(status.success());

    // 2. Load PCM.
    let pcm = vtb_asr::audio::read_wav_16k_mono(Path::new(wav)).unwrap();
    println!("PCM {} 采样 ({:.1}s)", pcm.len(), pcm.len() as f64 / 16000.0);

    // 3. Show VAD segmentation.
    let spans = detect_spans(&pcm, VadConfig::default());
    println!("\n=== VAD 分句 ({} 段) ===", spans.len());
    for (i, s) in spans.iter().enumerate() {
        println!("  段{i}: {:.1}-{:.1}s ({:.1}s)", s.start_ms() as f64/1000.0, s.end_ms() as f64/1000.0, (s.end_ms()-s.start_ms()) as f64/1000.0);
    }
    if spans.is_empty() {
        println!("  ⚠️  VAD 未检出语音（可能是纯音乐/噪声，能量阈值需调整）");
    }

    // 4. Transcribe.
    let model = std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache/vtb-toolkit/ggml-tiny.bin")).unwrap();
    if !model.exists() {
        eprintln!("无模型 {}", model.display());
        return;
    }
    let engine = Arc::new(WhisperEngine::new(&model, None).unwrap());
    println!("\n=== 转写中 (whisper tiny)… ===");
    let t0 = Instant::now();
    let segs = transcribe_buffer(engine, &pcm, StreamingConfig::default()).await;
    let elapsed = t0.elapsed();
    println!("转写完成 {} 段, 耗时 {:.1}s (RTF {:.2})", segs.len(), elapsed.as_secs_f64(),
        elapsed.as_secs_f64() / (pcm.len() as f64 / 16000.0));
    for s in &segs {
        println!("  [{:.1}-{:.1}s]{} {}", s.start_ms as f64/1000.0, s.end_ms as f64/1000.0,
            s.lang.as_deref().map(|l| format!(" ({l})")).unwrap_or_default(), s.text);
    }
    let _ = Duration::from_secs(0);

    if segs.is_empty() {
        println!("⚠️  无转写结果");
    } else if segs.iter().any(|s| !s.text.trim().is_empty()) {
        println!("✅ ASR 产出非空转写");
    }
}
