//! Consumers of hotword tables: whisper initial prompt + translator pairs.

use crate::types::HotwordTable;

/// Build whisper's `initial_prompt` from tables: a vocabulary sentence
/// nudging recognition toward these spellings. Whisper only reads ~224
/// tokens of prompt, so the output is capped (terms first — they matter
/// more than aliases).
pub fn asr_initial_prompt(tables: &[&HotwordTable], max_chars: usize) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    // First pass: canonical terms. Single-character terms are skipped —
    // they anchor poorly and add decode noise (they still feed the
    // translation glossary).
    for t in tables {
        for e in &t.entries {
            if e.term.chars().count() >= 2 && seen.insert(e.term.to_lowercase()) {
                words.push(e.term.clone());
            }
        }
    }
    // Second pass: readings help pronunciation-to-text mapping.
    for t in tables {
        for e in &t.entries {
            if let Some(r) = &e.reading {
                if seen.insert(r.to_lowercase()) {
                    words.push(r.clone());
                }
            }
        }
    }
    if words.is_empty() {
        return String::new();
    }
    // Bare comma-joined vocabulary — no framing prefix: whisper sometimes
    // bleeds prompt text into transcripts, and a bare word list bleeds far
    // less than a sentence (observed live: "词汇表:" leaked into output).
    let mut out = String::new();
    for (i, w) in words.iter().enumerate() {
        let add = if i == 0 { w.clone() } else { format!(", {w}") };
        if out.chars().count() + add.chars().count() > max_chars {
            break;
        }
        out.push_str(&add);
    }
    out
}

/// Fixed-translation pairs for the translator's glossary:
/// `(source_term, target, note)`.
pub fn translation_pairs(tables: &[&HotwordTable]) -> Vec<(String, String, Option<String>)> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for t in tables {
        for e in &t.entries {
            if let Some(tr) = &e.translation {
                if seen.insert(e.term.to_lowercase()) {
                    out.push((e.term.clone(), tr.clone(), e.note.clone()));
                }
                // Aliases translate to the same target.
                for a in &e.aliases {
                    if seen.insert(a.to_lowercase()) {
                        out.push((a.clone(), tr.clone(), e.note.clone()));
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{HotwordEntry, HotwordTable};

    fn sample() -> HotwordTable {
        HotwordTable::new(
            "t",
            vec![
                {
                    let mut e = HotwordEntry::new("GPK");
                    e.reading = Some("鸡皮开".into());
                    e
                },
                {
                    let mut e = HotwordEntry::new("帕克");
                    e.translation = Some("Puck".into());
                    e.aliases = vec!["puck".into()];
                    e
                },
            ],
        )
    }

    #[test]
    fn prompt_contains_terms_and_readings() {
        let t = sample();
        let p = asr_initial_prompt(&[&t], 200);
        assert!(p.starts_with("GPK"));
        assert!(p.contains("帕克"));
        assert!(p.contains("鸡皮开"));
        assert!(!p.contains("词汇表"), "no framing prefix (prompt bleed)");
    }

    #[test]
    fn prompt_caps_length_and_dedups() {
        let entries = (0..200)
            .map(|i| HotwordEntry::new(format!("词条编号{i}")))
            .collect();
        let t = HotwordTable::new("big", entries);
        let p = asr_initial_prompt(&[&t, &t], 100);
        assert!(p.chars().count() <= 102, "len={}", p.chars().count());
    }

    #[test]
    fn single_char_terms_excluded_from_prompt() {
        let t = HotwordTable::new(
            "t",
            vec![HotwordEntry::new("寄"), HotwordEntry::new("GPK")],
        );
        let p = asr_initial_prompt(&[&t], 100);
        assert!(p.contains("GPK"));
        assert!(!p.contains("寄"));
    }

    #[test]
    fn empty_tables_empty_prompt() {
        let t = HotwordTable::new("e", vec![]);
        assert_eq!(asr_initial_prompt(&[&t], 100), "");
    }

    #[test]
    fn translation_pairs_include_aliases() {
        let t = sample();
        let pairs = translation_pairs(&[&t]);
        assert_eq!(pairs.len(), 2); // 帕克→Puck, puck→Puck; GPK 无译法
        assert!(pairs.iter().any(|(s, tr, _)| s == "帕克" && tr == "Puck"));
        assert!(pairs.iter().any(|(s, _, _)| s == "puck"));
    }
}
