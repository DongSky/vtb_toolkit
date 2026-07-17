//! Diagnostic: realtime subtitle path — exactly what the Tauri
//! `live_subtitle_start` command does: resolve stream → ffmpeg pipes
//! s16le audio → StreamingAsr feeds chunks → segments emitted live,
//! optionally translated. Verifies latency behavior of the adaptive VAD.

use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;

use tokio::io::AsyncReadExt;
use vtb_asr::engine::WhisperEngine;
use vtb_asr::streaming::{StreamingAsr, StreamingConfig};
use vtb_recorder::stream::{pick_best, qn, StreamApi};
use vtb_translate::backend::mock::MockBackend;
use vtb_translate::{OpenAiCompatBackend, LlmBackend, StreamerProfile, TranslateConfig, TranslatePipeline};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let short: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(320);
    let secs: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(45);

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36")
        .build().unwrap();
    let api = StreamApi::new(client);
    let real = api.room_status(short).await.unwrap().real_room_id;
    let streams = api.play_info(real, qn::FLUENT).await.unwrap();
    let best = pick_best(&streams).unwrap().clone();

    let model = std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache/vtb-toolkit/ggml-tiny.bin")).unwrap();
    let engine = Arc::new(WhisperEngine::new(&model, None).unwrap());
    let mut asr = StreamingAsr::new(engine, StreamingConfig::default());

    // Translator: real endpoint if available, else mock (path identical).
    let backend: Arc<dyn LlmBackend> = match (std::env::var("OPENAI_BASE_URL"), std::env::var("OPENAI_API_KEY"), std::env::var("OPENAI_MODEL")) {
        (Ok(b), Ok(k), Ok(m)) => { println!("翻译后端: {m} (真实)"); Arc::new(OpenAiCompatBackend::new(b, k, m)) }
        _ => { println!("翻译后端: mock"); Arc::new(MockBackend::echo()) }
    };
    let mut translator = TranslatePipeline::new(
        backend,
        StreamerProfile::default(),
        TranslateConfig { target_lang: "en".into(), ..Default::default() }, // zh 直播→en，避免同语言跳过
    );

    // ffmpeg pipe — same invocation as the Tauri command.
    let mut cmd = tokio::process::Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error"])
        .args(["-user_agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36"])
        .args(["-headers", "Referer: https://live.bilibili.com/\r\n"])
        .args(["-i", &best.url])
        .args(["-vn", "-ac", "1", "-ar", "16000", "-f", "s16le", "-"])
        .stdout(Stdio::piped())
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let mut child = cmd.spawn().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    println!("实时字幕运行 {secs}s (room {real})…\n");
    let t0 = Instant::now();
    let mut buf = vec![0u8; 16000]; // 0.5 s chunks
    let mut n_segments = 0;
    let mut latencies: Vec<f64> = Vec::new();

    while t0.elapsed().as_secs() < secs {
        match stdout.read_exact(&mut buf).await {
            Ok(_) => {
                let pcm: Vec<f32> = buf.chunks_exact(2)
                    .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32767.0)
                    .collect();
                for seg in asr.feed(&pcm).await {
                    n_segments += 1;
                    // Latency: wall time now vs segment end position in stream.
                    let lat = t0.elapsed().as_secs_f64() - seg.end_ms as f64 / 1000.0;
                    latencies.push(lat);
                    let translated = if translator.should_translate(&seg) {
                        translator.translate_segment(&seg).await.ok().map(|t| t.translated_text)
                    } else { None };
                    println!("[{:>5.1}s | 延迟{:>4.1}s]{} {}",
                        seg.end_ms as f64 / 1000.0, lat,
                        seg.lang.as_deref().map(|l| format!(" ({l})")).unwrap_or_default(),
                        seg.text);
                    if let Some(t) = translated {
                        println!("            ↳ {t}");
                    }
                }
            }
            Err(e) => { println!("流结束: {e}"); break; }
        }
    }
    let _ = child.start_kill();

    println!("\n=== 统计 ===");
    println!("段数: {n_segments}");
    if !latencies.is_empty() {
        let avg = latencies.iter().sum::<f64>() / latencies.len() as f64;
        println!("平均出字延迟: {avg:.1}s (含说话时长)");
    }
    if n_segments > 0 {
        println!("✅ 实时字幕/同传路径跑通");
    } else {
        println!("⚠️  无段产出（该时段可能无人声）");
    }
}
