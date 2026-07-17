//! Prompt assembly for LLM translation.
//!
//! Design (2025/26 best practice for simultaneous LLM translation):
//! - System prompt: role, target language, streamer context, full glossary
//!   (pinned terminology), reference translations as style few-shots.
//! - User turn: a short rolling window of recent source lines for context,
//!   then the line to translate. The model must return ONLY the translation
//!   of the last line — keeps latency low and output parseable.

use crate::glossary::StreamerProfile;

/// Human-readable language names keyed by tag.
pub fn lang_name(tag: &str) -> &'static str {
    match tag {
        "zh" => "Simplified Chinese",
        "ja" => "Japanese",
        "en" => "English",
        _ => "the target language",
    }
}

/// Build the system prompt for a translation session.
pub fn build_system_prompt(profile: &StreamerProfile, target_lang: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "You are a professional simultaneous interpreter for a VTuber live \
         stream. Translate each line the streamer says into {}. Preserve the \
         speaker's tone and energy; keep internet slang natural; never add \
         explanations or romanization. Output ONLY the translation text.\n",
        lang_name(target_lang)
    ));

    if !profile.name.is_empty() {
        out.push_str(&format!("\nStreamer: {}\n", profile.name));
    }
    if !profile.description.is_empty() {
        out.push_str(&format!("Context: {}\n", profile.description));
    }

    if !profile.glossary.entries.is_empty() {
        out.push_str(
            "\nTerminology (ALWAYS translate these terms exactly as given):\n",
        );
        for e in &profile.glossary.entries {
            match &e.note {
                Some(n) => out.push_str(&format!("- {} => {} ({})\n", e.source, e.target, n)),
                None => out.push_str(&format!("- {} => {}\n", e.source, e.target)),
            }
        }
    }

    if !profile.references.is_empty() {
        out.push_str(
            "\nReference translations (match this style and register):\n",
        );
        for r in &profile.references {
            out.push_str(&format!("- 「{}」 => 「{}」\n", r.source, r.target));
        }
    }
    out
}

/// Build the user message: recent context lines + the line to translate.
pub fn build_user_prompt(history: &[String], line: &str) -> String {
    if history.is_empty() {
        return format!("Translate this line:\n{line}");
    }
    let ctx = history.join("\n");
    format!(
        "Recent lines (context only, do NOT translate):\n{ctx}\n\nTranslate ONLY this line:\n{line}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glossary::{Glossary, GlossaryEntry, ReferencePair};

    fn profile() -> StreamerProfile {
        StreamerProfile {
            name: "星街すいせい".into(),
            description: "偶像系VTuber，说话有力".into(),
            glossary: Glossary {
                entries: vec![GlossaryEntry {
                    source: "すいちゃん".into(),
                    target: "彗酱".into(),
                    note: Some("昵称".into()),
                }],
            },
            references: vec![ReferencePair {
                source: "今日も一日頑張るぞい".into(),
                target: "今天也要加油鸭".into(),
            }],
        }
    }

    #[test]
    fn system_prompt_contains_all_sections() {
        let p = build_system_prompt(&profile(), "zh");
        assert!(p.contains("Simplified Chinese"));
        assert!(p.contains("星街すいせい"));
        assert!(p.contains("偶像系VTuber"));
        assert!(p.contains("すいちゃん => 彗酱 (昵称)"));
        assert!(p.contains("今天也要加油鸭"));
        assert!(p.contains("Output ONLY the translation"));
    }

    #[test]
    fn system_prompt_minimal_profile() {
        let p = build_system_prompt(&StreamerProfile::default(), "en");
        assert!(p.contains("English"));
        assert!(!p.contains("Terminology"));
        assert!(!p.contains("Reference translations"));
    }

    #[test]
    fn user_prompt_without_history() {
        let p = build_user_prompt(&[], "こんにちは");
        assert!(p.starts_with("Translate this line:"));
        assert!(p.contains("こんにちは"));
    }

    #[test]
    fn user_prompt_with_history_marks_context() {
        let history = vec!["前の行".to_string(), "その前".to_string()];
        let p = build_user_prompt(&history, "今の行");
        assert!(p.contains("context only"));
        assert!(p.contains("前の行"));
        assert!(p.contains("Translate ONLY this line:\n今の行"));
    }

    #[test]
    fn lang_names() {
        assert_eq!(lang_name("zh"), "Simplified Chinese");
        assert_eq!(lang_name("ja"), "Japanese");
        assert_eq!(lang_name("xx"), "the target language");
    }
}
