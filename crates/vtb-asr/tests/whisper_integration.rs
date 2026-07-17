//! Real whisper.cpp integration test (macOS).
//!
//! Downloads ggml-tiny (~75 MB, cached under ~/.cache/vtb-toolkit) and
//! transcribes speech synthesized with the macOS `say` command.
//!
//! Run: `cargo test -p vtb-asr --test whisper_integration -- --ignored --nocapture`

#![cfg(feature = "whisper")]

use std::path::PathBuf;
use std::sync::Arc;
use vtb_asr::engine::WhisperEngine;
use vtb_asr::streaming::{transcribe_buffer, StreamingConfig};

const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin";

fn model_path() -> PathBuf {
    let cache = dirs_cache().join("vtb-toolkit");
    std::fs::create_dir_all(&cache).unwrap();
    cache.join("ggml-tiny.bin")
}

fn dirs_cache() -> PathBuf {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".cache"))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

async fn ensure_model() -> PathBuf {
    let path = model_path();
    if !path.exists() {
        eprintln!("downloading ggml-tiny model...");
        let bytes = reqwest::get(MODEL_URL).await.unwrap().bytes().await.unwrap();
        assert!(bytes.len() > 10_000_000, "model download too small");
        std::fs::write(&path, &bytes).unwrap();
    }
    path
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "downloads model; needs macOS `say` + ffmpeg"]
async fn whisper_transcribes_synthesized_english() {
    let dir = tempfile::tempdir().unwrap();
    let aiff = dir.path().join("speech.aiff");

    // Synthesize English speech.
    let st = tokio::process::Command::new("say")
        .args(["-o"])
        .arg(&aiff)
        .args(["Hello world. This is a test of the speech recognition system."])
        .status()
        .await
        .expect("`say` available on macOS");
    assert!(st.success());

    let pcm = vtb_asr::audio::extract_audio_ffmpeg(std::path::Path::new("ffmpeg"), &aiff)
        .await
        .unwrap();
    assert!(pcm.len() > 16_000, "synthesized audio too short");

    let model = ensure_model().await;
    let engine = Arc::new(WhisperEngine::new(&model, Some("en".into())).unwrap());
    let segs = transcribe_buffer(engine, &pcm, StreamingConfig::default()).await;

    assert!(!segs.is_empty(), "no segments transcribed");
    let all: String = segs
        .iter()
        .map(|s| s.text.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    eprintln!("transcribed: {all}");
    // The tiny model may drop very short leading words; require the core
    // sentence content.
    assert!(all.contains("test"), "expected 'test' in: {all}");
    assert!(
        all.contains("speech") || all.contains("recognition"),
        "expected 'speech'/'recognition' in: {all}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "downloads model; needs macOS `say` + ffmpeg"]
async fn whisper_auto_detects_language() {
    let dir = tempfile::tempdir().unwrap();
    let aiff = dir.path().join("zh.aiff");
    // Use a Chinese voice if available; fall back gracefully.
    let st = tokio::process::Command::new("say")
        .args(["-v", "Tingting", "-o"])
        .arg(&aiff)
        .args(["今天天气真不错，我们一起去公园散步吧。"])
        .status()
        .await
        .unwrap();
    if !st.success() {
        eprintln!("Tingting voice unavailable; skipping");
        return;
    }

    let pcm = vtb_asr::audio::extract_audio_ffmpeg(std::path::Path::new("ffmpeg"), &aiff)
        .await
        .unwrap();
    let model = ensure_model().await;
    let engine = Arc::new(WhisperEngine::new(&model, None).unwrap()); // auto
    let segs = transcribe_buffer(engine, &pcm, StreamingConfig::default()).await;
    assert!(!segs.is_empty());
    eprintln!("lang={:?} text={}", segs[0].lang, segs[0].text);
    assert_eq!(segs[0].lang.as_deref(), Some("zh"));
}
