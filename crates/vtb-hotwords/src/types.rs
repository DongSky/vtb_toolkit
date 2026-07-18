//! The unified hotword table format.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One normalized term.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotwordEntry {
    /// The term as it should be recognized/written, e.g. "GPK".
    pub term: String,
    /// Alternative spellings / aliases, e.g. ["gpk", "Gpk"].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// Reading hint (pinyin/kana) when pronunciation differs from text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading: Option<String>,
    /// Fixed translation for the translator's glossary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation: Option<String>,
    /// Free category, e.g. "选手" / "英雄" / "梗".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Free note shown to humans and the translator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl HotwordEntry {
    pub fn new(term: impl Into<String>) -> Self {
        Self {
            term: term.into(),
            aliases: vec![],
            reading: None,
            translation: None,
            category: None,
            note: None,
        }
    }

    /// Merge `other` into `self`: non-empty fields of `other` win; aliases
    /// union (deduped, order-preserving).
    pub fn absorb(&mut self, other: &HotwordEntry) {
        for a in &other.aliases {
            if !self.aliases.iter().any(|x| x.eq_ignore_ascii_case(a)) {
                self.aliases.push(a.clone());
            }
        }
        if other.reading.is_some() {
            self.reading = other.reading.clone();
        }
        if other.translation.is_some() {
            self.translation = other.translation.clone();
        }
        if other.category.is_some() {
            self.category = other.category.clone();
        }
        if other.note.is_some() {
            self.note = other.note.clone();
        }
    }
}

/// A named table of hotwords.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HotwordTable {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub entries: Vec<HotwordEntry>,
    pub updated_at: DateTime<Utc>,
}

impl HotwordTable {
    pub fn new(name: impl Into<String>, entries: Vec<HotwordEntry>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            entries: dedup_entries(entries),
            updated_at: Utc::now(),
        }
    }

    /// Merge several tables into a new one. Later tables' non-empty fields
    /// override earlier ones; terms dedup case-insensitively.
    pub fn merge(name: impl Into<String>, tables: &[&HotwordTable]) -> Self {
        let mut all = Vec::new();
        for t in tables {
            all.extend(t.entries.iter().cloned());
        }
        let mut merged = Self::new(name, all);
        merged.description = tables
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(" + ");
        merged
    }
}

/// Dedup by case-insensitive term, absorbing later duplicates' fields.
pub fn dedup_entries(entries: Vec<HotwordEntry>) -> Vec<HotwordEntry> {
    let mut out: Vec<HotwordEntry> = Vec::new();
    for mut e in entries {
        e.term = e.term.trim().to_string();
        if e.term.is_empty() {
            continue;
        }
        if let Some(existing) = out
            .iter_mut()
            .find(|x| x.term.eq_ignore_ascii_case(&e.term))
        {
            existing.absorb(&e);
        } else {
            out.push(e);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_merges_fields_case_insensitively() {
        let mut a = HotwordEntry::new("GPK");
        a.category = Some("选手".into());
        let mut b = HotwordEntry::new("gpk");
        b.translation = Some("GPK".into());
        b.aliases = vec!["Gpk".into()];
        let out = dedup_entries(vec![a, b, HotwordEntry::new("  "), HotwordEntry::new("帕克")]);
        assert_eq!(out.len(), 2);
        let gpk = &out[0];
        assert_eq!(gpk.term, "GPK");
        assert_eq!(gpk.category.as_deref(), Some("选手"));
        assert_eq!(gpk.translation.as_deref(), Some("GPK"));
        assert_eq!(gpk.aliases, vec!["Gpk"]);
    }

    #[test]
    fn merge_tables_overrides_and_names() {
        let t1 = HotwordTable::new(
            "dota",
            vec![{
                let mut e = HotwordEntry::new("帕克");
                e.translation = Some("Puck".into());
                e
            }],
        );
        let t2 = HotwordTable::new(
            "选手",
            vec![
                HotwordEntry::new("GPK"),
                {
                    let mut e = HotwordEntry::new("帕克");
                    e.note = Some("英雄名".into());
                    e
                },
            ],
        );
        let m = HotwordTable::merge("合并", &[&t1, &t2]);
        assert_eq!(m.entries.len(), 2);
        assert_eq!(m.description, "dota + 选手");
        let pk = m.entries.iter().find(|e| e.term == "帕克").unwrap();
        assert_eq!(pk.translation.as_deref(), Some("Puck")); // kept
        assert_eq!(pk.note.as_deref(), Some("英雄名")); // absorbed
    }
}
