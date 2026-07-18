//! LLM-powered normalization of arbitrary natural-language hotword input.
//!
//! Free prose like "我们队打野是GPK,大家老打成gpk或者鸡皮开;英雄帕克翻译成
//! Puck别音译" can't be handled by rules — a small LLM call restructures it
//! into the unified entry format. Falls back to [`crate::parse_rules`].

use crate::error::{HotwordError, Result};
use crate::types::{dedup_entries, HotwordEntry};
use async_trait::async_trait;

/// Minimal completion interface so this crate stays independent of any
/// specific LLM client (the app adapts its vtb-translate backend to this).
#[async_trait]
pub trait TextCompleter: Send + Sync {
    async fn complete(&self, system: &str, user: &str) -> std::result::Result<String, String>;
}

pub const NORMALIZE_SYSTEM: &str = r#"You convert a streamer's free-form terminology notes into a JSON array of hotword entries. Each entry:
{"term": "canonical写法", "aliases": ["别名/错拼"], "reading": "读音提示或null", "translation": "固定译法或null", "category": "类别或null", "note": "备注或null"}

Rules:
- term is how the word should appear in subtitles (correct spelling/casing).
- Common mishearings/misspellings the user mentions go into aliases.
- If the user gives a fixed translation ("X翻译成Y"/"X=Y"), set translation.
- Keep categories short (选手/英雄/梗/称呼/地名/歌名/其他).
- Do NOT invent terms not present in the input.
- Output ONLY the JSON array, no prose, no code fences."#;

/// Extract a JSON array from a possibly-fenced/prose-wrapped response.
pub fn extract_json_array(text: &str) -> Option<&str> {
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    (end > start).then(|| &text[start..=end])
}

/// Parse the model output into entries (tolerant of fences/prose).
pub fn parse_llm_output(text: &str) -> Result<Vec<HotwordEntry>> {
    let json = extract_json_array(text)
        .ok_or_else(|| HotwordError::Normalize("响应中没有 JSON 数组".into()))?;
    let entries: Vec<HotwordEntry> = serde_json::from_str(json)?;
    Ok(dedup_entries(entries))
}

/// Normalize free text via the LLM; falls back to the rule parser when the
/// call or parsing fails (feature keeps working without an API key).
pub async fn normalize_with_llm(
    completer: &dyn TextCompleter,
    raw: &str,
) -> (Vec<HotwordEntry>, bool) {
    match completer.complete(NORMALIZE_SYSTEM, raw).await {
        Ok(out) => match parse_llm_output(&out) {
            Ok(entries) if !entries.is_empty() => (entries, true),
            Ok(_) => (crate::parse::parse_rules(raw), false),
            Err(e) => {
                tracing::warn!("LLM 输出解析失败,回退规则解析: {e}");
                (crate::parse::parse_rules(raw), false)
            }
        },
        Err(e) => {
            tracing::warn!("LLM 规整失败,回退规则解析: {e}");
            (crate::parse::parse_rules(raw), false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scripted(std::sync::Mutex<Vec<std::result::Result<String, String>>>);

    #[async_trait]
    impl TextCompleter for Scripted {
        async fn complete(
            &self,
            _s: &str,
            _u: &str,
        ) -> std::result::Result<String, String> {
            self.0.lock().unwrap().remove(0)
        }
    }

    #[test]
    fn extracts_from_fenced_output() {
        let fenced = "Here you go:\n```json\n[{\"term\":\"GPK\"}]\n```";
        let out = parse_llm_output(fenced).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].term, "GPK");
    }

    #[test]
    fn rejects_no_array() {
        assert!(parse_llm_output("no json here").is_err());
        assert!(parse_llm_output("{\"term\":\"x\"}").is_err());
    }

    #[tokio::test]
    async fn llm_success_path() {
        let c = Scripted(std::sync::Mutex::new(vec![Ok(
            r#"[{"term":"GPK","aliases":["gpk","鸡皮开"],"category":"选手"},
                {"term":"帕克","translation":"Puck","category":"英雄"}]"#
                .into(),
        )]));
        let (entries, used_llm) = normalize_with_llm(&c, "whatever").await;
        assert!(used_llm);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].aliases, vec!["gpk", "鸡皮开"]);
        assert_eq!(entries[1].translation.as_deref(), Some("Puck"));
    }

    #[tokio::test]
    async fn llm_failure_falls_back_to_rules() {
        let c = Scripted(std::sync::Mutex::new(vec![Err("api down".into())]));
        let (entries, used_llm) = normalize_with_llm(&c, "GPK\n帕克=Puck").await;
        assert!(!used_llm);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].translation.as_deref(), Some("Puck"));
    }

    #[tokio::test]
    async fn garbage_llm_output_falls_back() {
        let c = Scripted(std::sync::Mutex::new(vec![Ok("I cannot help".into())]));
        let (entries, used_llm) = normalize_with_llm(&c, "GPK、Ame").await;
        assert!(!used_llm);
        assert_eq!(entries.len(), 2);
    }
}
