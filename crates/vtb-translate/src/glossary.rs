//! Glossary and per-streamer translation profiles.
//!
//! A [`StreamerProfile`] carries everything that personalizes translation
//! for one streamer: pinned terminology (names, memes, catchphrases) and
//! reference translations (source→target pairs demonstrating desired style).

use serde::{Deserialize, Serialize};
use std::path::Path;

/// One pinned term: whenever `source` appears, translate it as `target`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlossaryEntry {
    pub source: String,
    pub target: String,
    /// Optional note shown to the model ("角色名", "梗，勿直译" ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A source/target pair demonstrating desired translation style.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReferencePair {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Glossary {
    pub entries: Vec<GlossaryEntry>,
}

impl Glossary {
    /// Entries whose source string occurs in `text` (substring match —
    /// suits ja/zh where word boundaries are unreliable).
    pub fn matches<'a>(&'a self, text: &str) -> Vec<&'a GlossaryEntry> {
        self.entries
            .iter()
            .filter(|e| !e.source.is_empty() && text.contains(&e.source))
            .collect()
    }
}

/// Everything that personalizes translation for one streamer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StreamerProfile {
    /// Display name, e.g. "星街すいせい".
    pub name: String,
    /// Free-form context: who the streamer is, speech style, fandom terms.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub glossary: Glossary,
    /// Reference translations provided by the user (few-shot examples).
    #[serde(default)]
    pub references: Vec<ReferencePair>,
}

impl StreamerProfile {
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glossary() -> Glossary {
        Glossary {
            entries: vec![
                GlossaryEntry {
                    source: "すいちゃん".into(),
                    target: "彗酱".into(),
                    note: Some("昵称".into()),
                },
                GlossaryEntry {
                    source: "スイセイ".into(),
                    target: "彗星".into(),
                    note: None,
                },
            ],
        }
    }

    #[test]
    fn matches_finds_substrings() {
        let g = glossary();
        let hits = g.matches("今日もすいちゃんは可愛い");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].target, "彗酱");
    }

    #[test]
    fn matches_none_when_absent() {
        assert!(glossary().matches("全然関係ない話").is_empty());
    }

    #[test]
    fn empty_source_never_matches() {
        let g = Glossary {
            entries: vec![GlossaryEntry {
                source: "".into(),
                target: "x".into(),
                note: None,
            }],
        };
        assert!(g.matches("anything").is_empty());
    }

    #[test]
    fn profile_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile.json");
        let profile = StreamerProfile {
            name: "テスト".into(),
            description: "关西腔，喜欢用双关".into(),
            glossary: glossary(),
            references: vec![ReferencePair {
                source: "草".into(),
                target: "笑死".into(),
            }],
        };
        profile.save(&path).unwrap();
        let loaded = StreamerProfile::load(&path).unwrap();
        assert_eq!(loaded, profile);
    }
}
