//! Diagnostic: danmaku auto-translation end-to-end with real chat.
//! Connects a live room, script-filters chat lines with
//! `should_translate_danmaku`, and (when OPENAI_* env is set) translates
//! them with the real LLM — printing what an overlay/UI would show.
//!
//! Usage: cargo run -p vtb-e2e --bin diag_danmaku_translate -- <room> <secs> [target_lang]

use std::sync::Arc;
use std::time::Duration;
use vtb_danmaku::api::BiliApi;
use vtb_danmaku::{spawn_managed, DanmakuClientConfig, ManagedEvent};
use vtb_translate::danmaku::should_translate_danmaku;
use vtb_translate::{OpenAiCompatBackend, StreamerProfile, TranslateConfig, TranslatePipeline};

#[tokio::main]
async fn main() {
    let room: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(320);
    let secs: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(40);
    let target = std::env::args().nth(3).unwrap_or_else(|| "ja".into());

    let mut pipeline = match (
        std::env::var("OPENAI_BASE_URL"),
        std::env::var("OPENAI_API_KEY"),
        std::env::var("OPENAI_MODEL"),
    ) {
        (Ok(base), Ok(key), Ok(model)) => {
            println!("LLM: {model} @ {base} → {target}");
            Some(TranslatePipeline::new(
                Arc::new(OpenAiCompatBackend::new(base, key, model)),
                StreamerProfile::default(),
                TranslateConfig {
                    target_lang: target.clone(),
                    context_lines: 6,
                    min_chars: 2,
                    skip_same_lang: false,
                },
            ))
        }
        _ => {
            println!("⚠️  未设置 OPENAI_* 环境变量，只演示过滤，不做真实翻译");
            None
        }
    };

    let api = BiliApi::default_client().unwrap();
    let real = api.room_init(room).await.unwrap().real_room_id;
    println!("采集 {secs}s 弹幕 (room {room} → real {real})…");
    let (mut rx, stop) = spawn_managed(
        BiliApi::default_client().unwrap(),
        DanmakuClientConfig::anonymous(real),
    );

    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    let (mut total, mut picked, mut translated) = (0u32, 0u32, 0u32);
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            ev = rx.recv() => match ev {
                Some(ManagedEvent::Live(live)) => {
                    let text = match &live {
                        vtb_common::LiveEvent::Danmaku(d) if d.emoticon.is_none() => d.text.clone(),
                        vtb_common::LiveEvent::SuperChat(s) => s.text.clone(),
                        _ => continue,
                    };
                    total += 1;
                    if !should_translate_danmaku(&text, &target) {
                        println!("  跳过: {text}");
                        continue;
                    }
                    picked += 1;
                    match &mut pipeline {
                        Some(p) => {
                            let seg = vtb_common::TranscriptSegment {
                                start_ms: 0, end_ms: 0, text: text.clone(),
                                lang: None, is_final: true,
                            };
                            match p.translate_segment(&seg).await {
                                Ok(t) => {
                                    translated += 1;
                                    println!("✅ {text}  →  {}", t.translated_text);
                                }
                                Err(e) => println!("❌ {text}  翻译失败: {e}"),
                            }
                        }
                        None => println!("→ 需翻译: {text}"),
                    }
                }
                Some(_) => {}
                None => break,
            }
        }
    }
    let _ = stop.send(true);
    println!("\n弹幕/SC {total} 条，判定需翻译 {picked} 条，成功翻译 {translated} 条");
    if total > 0 && picked == 0 && target != "zh" {
        println!("⚠️  全部被过滤（正常：目标语与弹幕同语种时才应大量跳过）");
    }
}
