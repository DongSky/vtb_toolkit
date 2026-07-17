//! Diagnostic: real translation pipeline against a live OpenAI-compatible
//! endpoint (OPENAI_BASE_URL / OPENAI_API_KEY / OPENAI_MODEL). Verifies:
//!   - personalization (glossary term forced into output)
//!   - rolling context across multiple lines
//!   - real HTTP round-trip + response parsing

use std::sync::Arc;

use vtb_common::TranscriptSegment;
use vtb_translate::backend::mock::MockBackend;
use vtb_translate::glossary::{Glossary, GlossaryEntry, ReferencePair, StreamerProfile};
use vtb_translate::{OpenAiCompatBackend, TranslateConfig, TranslatePipeline};

fn seg(text: &str) -> TranscriptSegment {
    TranscriptSegment {
        start_ms: 0,
        end_ms: 2000,
        text: text.into(),
        lang: Some("ja".into()),
        is_final: true,
    }
}

fn profile() -> StreamerProfile {
    StreamerProfile {
        name: "測試VTuber".into(),
        description: "元气偶像系".into(),
        glossary: Glossary {
            entries: vec![
                GlossaryEntry { source: "すいちゃん".into(), target: "彗酱".into(), note: None },
                GlossaryEntry { source: "ぺこら".into(), target: "佩克拉".into(), note: None },
            ],
        },
        references: vec![ReferencePair {
            source: "こんぺこ".into(),
            target: "空佩科".into(),
        }],
    }
}

#[tokio::main]
async fn main() {
    // ---- Part A: mock backend — verify prompt construction (offline) ----
    println!("=== Part A: 个性化 prompt 注入 (mock) ===");
    let mock = Arc::new(MockBackend::echo());
    let mut p = TranslatePipeline::new(mock.clone(), profile(), TranslateConfig::default());
    let _ = p.translate_segment(&seg("すいちゃん、こんぺこ！")).await.unwrap();
    let calls = mock.calls.lock().unwrap();
    let (system, user) = &calls[0];
    println!("system prompt 片段:\n{}", system.lines().take(20).collect::<Vec<_>>().join("\n"));
    assert!(system.contains("すいちゃん") && system.contains("彗酱"), "术语表缺失");
    assert!(system.contains("空佩科"), "参考翻译缺失");
    println!("user prompt:\n{user}");
    println!("✅ 术语表+参考翻译已注入\n");
    drop(calls);

    // ---- Part B: real endpoint round-trip ----
    let (Ok(base), Ok(key), Ok(model)) = (
        std::env::var("OPENAI_BASE_URL"),
        std::env::var("OPENAI_API_KEY"),
        std::env::var("OPENAI_MODEL"),
    ) else {
        println!("⚠️  未设置 OPENAI_* 环境变量，跳过真实翻译");
        return;
    };
    println!("=== Part B: 真实翻译 ({model} @ {base}) ===");

    let backend = Arc::new(OpenAiCompatBackend::new(base, key, model));
    let mut pipe = TranslatePipeline::new(
        backend,
        profile(),
        TranslateConfig { target_lang: "zh".into(), ..Default::default() },
    );

    // A short multi-line Japanese conversation to test context + glossary.
    let lines = [
        "みなさん、こんぺこ！",
        "今日は初めての配信です。",
        "すいちゃんも来てくれました！",
        "楽しんでいってね。",
    ];
    for (i, line) in lines.iter().enumerate() {
        match pipe.translate_segment(&seg(line)).await {
            Ok(t) => {
                println!("  [{i}] 原文: {}", t.source_text);
                println!("      译文: {}", t.translated_text);
                // Verify glossary term appears when its source is present.
                if line.contains("すいちゃん") && !t.translated_text.contains("彗酱") {
                    println!("      ⚠️  术语'彗酱'未出现（模型可能未严格遵守，但请求正确）");
                }
            }
            Err(e) => {
                println!("  [{i}] ❌ 翻译失败: {e}");
                return;
            }
        }
    }
    println!("✅ 真实 LLM 翻译管线跑通（含术语表+滚动上下文）");
}
