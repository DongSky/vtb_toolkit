//! Diagnostic: hotword tables end-to-end.
//! Part A — real-LLM normalization of messy natural language.
//! Part B — ASR accuracy with vs. without the hotword initial prompt on a
//! real recording (the GPK mis-recognition case).

use std::path::Path;
use std::sync::Arc;
use vtb_asr::engine::WhisperEngine;
use vtb_asr::streaming::{transcribe_buffer, StreamingConfig};
use vtb_hotwords::{asr_initial_prompt, normalize_with_llm, HotwordTable, TableStore};

struct OpenAiCompleter(vtb_translate::OpenAiCompatBackend);

#[async_trait::async_trait]
impl vtb_hotwords::TextCompleter for OpenAiCompleter {
    async fn complete(&self, system: &str, user: &str) -> Result<String, String> {
        use vtb_translate::LlmBackend;
        self.0.complete(system, user).await.map_err(|e| e.to_string())
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    // ---------- Part A: LLM normalization ----------
    println!("═══ Part A: 自然语言 → 统一热词表 ═══");
    let raw = "我们队打野选手叫GPK,直播里经常被识别成gpt或者鸡皮开,注意别写错。\
               然后英雄方面:帕克要翻译成Puck别音译,敌法师简称AM英文是Anti-Mage,\
               还有个梗叫'寄'就是完蛋的意思不用翻译。选手Ame和XinQ也常出现。";
    println!("输入(乱格式自然语言):\n{raw}\n");

    let entries = match (
        std::env::var("OPENAI_BASE_URL"),
        std::env::var("OPENAI_API_KEY"),
        std::env::var("OPENAI_MODEL"),
    ) {
        (Ok(b), Ok(k), Ok(m)) => {
            let completer =
                OpenAiCompleter(vtb_translate::OpenAiCompatBackend::new(b, k, m));
            // Surface raw errors (normalize_with_llm only warns via tracing).
            use vtb_hotwords::TextCompleter;
            match completer.complete("ping: reply with []", "[]").await {
                Ok(_) => println!("LLM 连通性: ✅"),
                Err(e) => println!("LLM 连通性: ❌ {e}"),
            }
            let (entries, used_llm) = normalize_with_llm(&completer, raw).await;
            println!("规整方式: {}", if used_llm { "LLM ✅" } else { "规则回退 ⚠️" });
            entries
        }
        _ => {
            println!("无 OPENAI_* 环境变量,用规则解析");
            vtb_hotwords::parse_rules(raw)
        }
    };
    println!("规整结果 {} 个词条:", entries.len());
    for e in &entries {
        println!(
            "  {} | 别名{:?} | 译:{} | 类:{} | 注:{}",
            e.term,
            e.aliases,
            e.translation.as_deref().unwrap_or("-"),
            e.category.as_deref().unwrap_or("-"),
            e.note.as_deref().unwrap_or("-"),
        );
    }

    // Save through the real store (management path).
    let dir = std::env::temp_dir().join("vtb-hotwords-diag");
    let _ = std::fs::remove_dir_all(&dir);
    let store = TableStore::open(&dir).unwrap();
    let table = HotwordTable::new("DOTA测试", entries);
    store.save(&table).unwrap();
    let loaded = store.load("DOTA测试").unwrap();
    println!("存储 roundtrip: {} 词条 ✅", loaded.entries.len());

    // ---------- Part B: ASR effect ----------
    println!("\n═══ Part B: 热词对 ASR 的实际影响(small 模型,生产推荐配置)═══");
    let rec = "/Users/dongsky/Documents/vtb_toolkit_output/room24158116-fixed/rec.flv";
    if !Path::new(rec).exists() {
        println!("测试录音不存在,跳过 Part B");
        return;
    }
    let model = std::env::var_os("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".cache/vtb-toolkit/ggml-small.bin"))
        .unwrap();
    if !model.exists() {
        println!("无 small 模型,跳过 Part B");
        return;
    }
    let pcm = vtb_asr::audio::extract_audio_ffmpeg(Path::new("ffmpeg"), Path::new(rec))
        .await
        .unwrap();

    let prompt = asr_initial_prompt(&[&loaded], 800);
    println!("initial_prompt: {prompt}\n");

    for (label, use_prompt) in [("无热词", false), ("有热词", true)] {
        let mut engine = WhisperEngine::new(&model, None).unwrap();
        if use_prompt {
            engine = engine.with_initial_prompt(prompt.clone());
        }
        let segs =
            transcribe_buffer(Arc::new(engine), &pcm, StreamingConfig::default()).await;
        println!("── {label} ──");
        for s in &segs {
            println!("  [{:.1}s] {}", s.start_ms as f64 / 1000.0, s.text);
        }
    }
}
