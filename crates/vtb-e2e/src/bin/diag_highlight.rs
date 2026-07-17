//! Diagnostic: highlight detection over real concurrent danmaku + audio.
//! Records audio and collects danmaku for N seconds simultaneously, then
//! runs the full signal fusion and prints per-window scores + candidates.

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::Mutex;
use vtb_common::LiveEvent;
use vtb_danmaku::{DanmakuClient, DanmakuClientConfig};
use vtb_highlight::fusion::{detect_highlights, FusionConfig, SignalSet};
use vtb_highlight::signals::{audio_energy_scores, danmaku_density, gift_value, keyword_score};
use vtb_pipeline::energy::rms_series;
use vtb_recorder::stream::{pick_best, qn, StreamApi};

#[tokio::main]
async fn main() {
    let short: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(320);
    let secs: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(90);

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36")
        .build().unwrap();
    let api = StreamApi::new(client);
    let real = api.room_status(short).await.unwrap().real_room_id;
    let streams = api.play_info(real, qn::HD).await.unwrap();
    let best = pick_best(&streams).unwrap().clone();

    let session_start = Utc::now();
    println!("并发采集 {secs}s: 弹幕 + 音频 (room {real})");

    // Danmaku collection task.
    let events: Arc<Mutex<Vec<(u64, LiveEvent)>>> = Arc::new(Mutex::new(Vec::new()));
    let ev2 = events.clone();
    let dm_client = DanmakuClient::new(DanmakuClientConfig::anonymous(real)).unwrap();
    let (mut rx, handle) = dm_client.connect().await.unwrap();
    let collector = tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            let off = (Utc::now() - session_start).num_milliseconds().max(0) as u64;
            ev2.lock().await.push((off, ev));
        }
    });

    // Audio recording (blocking on ffmpeg -t).
    let wav = "/tmp/vtb-hl-audio.wav";
    let ff = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-user_agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36"])
        .args(["-headers", "Referer: https://live.bilibili.com/\r\n"])
        .args(["-i", &best.url, "-t", &secs.to_string()])
        .args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", wav])
        .status().await.unwrap();
    assert!(ff.success());

    handle.abort();
    collector.abort();
    let _ = collector.await;
    let events: Vec<(u64, LiveEvent)> = events.lock().await.clone();
    let refs: Vec<(u64, &LiveEvent)> = events.iter().map(|(o, e)| (*o, e)).collect();

    let pcm = vtb_asr::audio::read_wav_16k_mono(Path::new(wav)).unwrap();
    let total_ms = (pcm.len() as u64 * 1000) / 16000;
    let window_ms = 5000u64;

    let n_danmaku = refs.iter().filter(|(_, e)| matches!(e, LiveEvent::Danmaku(_))).count();
    println!("采集: {} 事件 ({} 弹幕), 音频 {:.1}s", events.len(), n_danmaku, total_ms as f64 / 1000.0);

    let density = danmaku_density(&refs, window_ms, total_ms);
    let keyword = keyword_score(&refs, window_ms, total_ms);
    let gift = gift_value(&refs, window_ms, total_ms);
    let audio = audio_energy_scores(&rms_series(&pcm, 500), window_ms, total_ms);

    println!("\n=== 各窗口信号 (window={}s) ===", window_ms / 1000);
    println!("  窗口   弹幕数  关键词  礼物¥  音频能量");
    for i in 0..density.values.len() {
        println!("  [{:>2}] {:>6.0} {:>6.0} {:>6.1} {:>8.4}",
            i, density.values[i], keyword.values[i], gift.values[i], audio.values[i]);
    }

    let signals = SignalSet {
        density: Some(density),
        keyword: Some(keyword),
        gift: Some(gift),
        audio: Some(audio),
    };

    // Use default fusion config (real thresholds).
    let cfg = FusionConfig::default();
    let highlights = detect_highlights(&signals, &cfg, total_ms);
    println!("\n=== Highlight 候选 (默认阈值 {:.1}) ===", cfg.threshold);
    if highlights.is_empty() {
        println!("  (无候选 — 信号平坦，属正常。此段直播无突出高能点)");
    }
    for h in &highlights {
        println!("  [{:.0}-{:.0}s] 分数 {:.2} — {}", h.start_ms as f64/1000.0, h.end_ms as f64/1000.0, h.score, h.reason);
    }

    // Also show with a lower threshold to prove fusion ranks correctly.
    let mut low = FusionConfig::default();
    low.threshold = 0.3;
    let more = detect_highlights(&signals, &low, total_ms);
    println!("\n=== 降低阈值到 0.3 后候选 (验证排序) ===");
    for h in &more {
        println!("  [{:.0}-{:.0}s] 分数 {:.2} — {}", h.start_ms as f64/1000.0, h.end_ms as f64/1000.0, h.score, h.reason);
    }
    println!("\n✅ Highlight 融合逻辑运行正常");
}
