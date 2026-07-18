//! Dependency-free rule parser for common hotword input formats.
//!
//! Handles, per line:
//! - bullets/numbering: `- x`, `1. x`, `• x`
//! - term with translation: `帕克=Puck`, `敌法师 -> Anti-Mage`, `恐鳄：Krogh`
//! - aliases in parentheses: `敌法师(AM, anti-mage)`
//! - inline note after `#` or `//`
//! - comma/顿号-separated bare term lists: `GPK、Ame, XinQ`
//!
//! Free prose it can't decode degrades to whole-line terms only when the
//! line is short; long prose lines are skipped (the LLM normalizer handles
//! those).

use crate::types::{dedup_entries, HotwordEntry};

const SEPS: &[&str] = &["=>", "->", "→", "＝", "=", "：", ":", "\t", "|"];

fn strip_bullet(line: &str) -> &str {
    let l = line.trim_start();
    let l = l
        .trim_start_matches(['-', '*', '•', '·'])
        .trim_start();
    // "1." / "1、" / "(1)" numbering.
    let mut chars = l.char_indices().peekable();
    let mut digits_end = 0;
    while let Some((i, c)) = chars.peek().copied() {
        if c.is_ascii_digit() {
            digits_end = i + c.len_utf8();
            chars.next();
        } else {
            break;
        }
    }
    if digits_end > 0 {
        let rest = &l[digits_end..];
        if let Some(r) = rest.strip_prefix(['.', '、', ')', '）']) {
            return r.trim_start();
        }
    }
    l
}

/// Split off `(alias1, alias2)` or `（…）` from the end of a term.
fn split_aliases(term: &str) -> (String, Vec<String>) {
    let t = term.trim();
    for (open, close) in [('(', ')'), ('（', '）')] {
        if let (Some(o), true) = (t.find(open), t.ends_with(close)) {
            let base = t[..o].trim().to_string();
            let inner = &t[o + open.len_utf8()..t.len() - close.len_utf8()];
            let aliases: Vec<String> = inner
                .split([',', '、', '，', '/'])
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if !base.is_empty() {
                return (base, aliases);
            }
        }
    }
    (t.to_string(), vec![])
}

fn parse_line(line: &str) -> Vec<HotwordEntry> {
    // Strip trailing comment.
    let mut body = line;
    for marker in ["#", "//"] {
        if let Some(i) = body.find(marker) {
            body = &body[..i];
        }
    }
    let note = {
        let after = line.strip_prefix(body).unwrap_or("");
        let n = after
            .trim_start_matches(['#', '/'])
            .trim()
            .to_string();
        (!n.is_empty()).then_some(n)
    };
    let body = strip_bullet(body).trim();
    if body.is_empty() {
        return vec![];
    }

    // term=translation form? (lhs must look like a term, not prose)
    for sep in SEPS {
        if let Some(i) = body.find(sep) {
            let (lhs, rhs) = (body[..i].trim(), body[i + sep.len()..].trim());
            if !lhs.is_empty() && !rhs.is_empty() && lhs.chars().count() <= 24 {
                let (term, aliases) = split_aliases(lhs);
                let mut e = HotwordEntry::new(term);
                e.aliases = aliases;
                e.translation = Some(rhs.to_string());
                e.note = note;
                return vec![e];
            }
        }
    }

    // Bare list: split on list separators.
    let parts: Vec<&str> = body
        .split([',', '、', '，', ';', '；'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() > 1 {
        return parts
            .into_iter()
            .map(|p| {
                let (term, aliases) = split_aliases(p);
                let mut e = HotwordEntry::new(term);
                e.aliases = aliases;
                e
            })
            .collect();
    }

    // Single token line. Skip long prose (LLM territory).
    if body.chars().count() > 24 {
        return vec![];
    }
    let (term, aliases) = split_aliases(body);
    let mut e = HotwordEntry::new(term);
    e.aliases = aliases;
    e.note = note;
    vec![e]
}

/// Parse arbitrary text with rules only. Always succeeds; unparseable
/// prose lines are simply skipped.
pub fn parse_rules(text: &str) -> Vec<HotwordEntry> {
    dedup_entries(text.lines().flat_map(parse_line).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bullets_numbering_and_bare_terms() {
        let text = "- GPK\n* Ame\n1. XinQ\n2、帕克\n• 老十一";
        let out = parse_rules(text);
        let terms: Vec<_> = out.iter().map(|e| e.term.as_str()).collect();
        assert_eq!(terms, vec!["GPK", "Ame", "XinQ", "帕克", "老十一"]);
    }

    #[test]
    fn translations_with_various_separators() {
        let text = "帕克=Puck\n敌法师 -> Anti-Mage\n恐鳄：Krogh\n斧王 → Axe";
        let out = parse_rules(text);
        assert_eq!(out.len(), 4);
        assert_eq!(out[0].translation.as_deref(), Some("Puck"));
        assert_eq!(out[1].term, "敌法师");
        assert_eq!(out[1].translation.as_deref(), Some("Anti-Mage"));
        assert_eq!(out[2].translation.as_deref(), Some("Krogh"));
        assert_eq!(out[3].translation.as_deref(), Some("Axe"));
    }

    #[test]
    fn aliases_in_parens() {
        let out = parse_rules("敌法师(AM, anti-mage)=Anti-Mage");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].term, "敌法师");
        assert_eq!(out[0].aliases, vec!["AM", "anti-mage"]);
        assert_eq!(out[0].translation.as_deref(), Some("Anti-Mage"));
    }

    #[test]
    fn comma_separated_lists() {
        let out = parse_rules("GPK、Ame，XinQ, Faith_bian");
        let terms: Vec<_> = out.iter().map(|e| e.term.as_str()).collect();
        assert_eq!(terms, vec!["GPK", "Ame", "XinQ", "Faith_bian"]);
    }

    #[test]
    fn comments_become_notes() {
        let out = parse_rules("GPK # 打野选手\n帕克=Puck // 英雄");
        assert_eq!(out[0].note.as_deref(), Some("打野选手"));
        assert_eq!(out[1].note.as_deref(), Some("英雄"));
    }

    #[test]
    fn long_prose_skipped_and_dupes_merged() {
        let text = "这是一段很长的自然语言描述规则解析器不应把它当成词条处理\nGPK\ngpk=GPK选手";
        let out = parse_rules(text);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].term, "GPK");
        assert_eq!(out[0].translation.as_deref(), Some("GPK选手"));
    }

    #[test]
    fn prose_with_separator_not_misparsed() {
        // Real regression: a long sentence containing "：" was parsed as
        // term=translation, poisoning the ASR prompt.
        let text = "然后英雄方面：帕克要翻译成Puck别音译,敌法师简称AM英文是Anti-Mage,还有个梗";
        let out = parse_rules(text);
        assert!(
            out.iter().all(|e| e.term.chars().count() <= 24),
            "prose leaked into terms: {out:?}"
        );
    }

    #[test]
    fn empty_input() {
        assert!(parse_rules("").is_empty());
        assert!(parse_rules("\n  \n").is_empty());
    }
}
