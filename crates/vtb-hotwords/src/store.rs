//! Table persistence: one JSON file per table in a directory, atomic
//! writes, plus merge/delete/rename management.

use crate::error::{HotwordError, Result};
use crate::types::HotwordTable;
use std::path::{Path, PathBuf};

pub struct TableStore {
    dir: PathBuf,
}

/// Filesystem-safe file stem for a table name (CJK preserved).
fn safe_stem(name: &str) -> Result<String> {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') || !c.is_ascii() {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .replace(' ', "_");
    if cleaned.is_empty() || cleaned.chars().all(|c| c == '_') {
        return Err(HotwordError::BadName(name.to_string()));
    }
    Ok(cleaned)
}

impl TableStore {
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn path_for(&self, name: &str) -> Result<PathBuf> {
        Ok(self.dir.join(format!("{}.json", safe_stem(name)?)))
    }

    /// Atomic save (tmp + rename); creates or overwrites.
    pub fn save(&self, table: &HotwordTable) -> Result<()> {
        let path = self.path_for(&table.name)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(table)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn load(&self, name: &str) -> Result<HotwordTable> {
        let path = self.path_for(name)?;
        let text = std::fs::read_to_string(&path)
            .map_err(|_| HotwordError::NotFound(name.to_string()))?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        let path = self.path_for(name)?;
        if !path.exists() {
            return Err(HotwordError::NotFound(name.to_string()));
        }
        std::fs::remove_file(path)?;
        Ok(())
    }

    /// All tables, sorted by name. Corrupt files are skipped with a warning.
    pub fn list(&self) -> Result<Vec<HotwordTable>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str::<HotwordTable>(&t).ok())
            {
                Some(t) => out.push(t),
                None => tracing::warn!("skipping corrupt hotword table {:?}", path),
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// Load several tables and save their merge as `new_name`.
    pub fn merge(&self, names: &[String], new_name: &str) -> Result<HotwordTable> {
        if names.is_empty() {
            return Err(HotwordError::BadName("空的合并列表".into()));
        }
        let loaded: Vec<HotwordTable> = names
            .iter()
            .map(|n| self.load(n))
            .collect::<Result<_>>()?;
        let refs: Vec<&HotwordTable> = loaded.iter().collect();
        let merged = HotwordTable::merge(new_name, &refs);
        self.save(&merged)?;
        Ok(merged)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HotwordEntry;

    fn table(name: &str, terms: &[&str]) -> HotwordTable {
        HotwordTable::new(
            name,
            terms.iter().map(|t| HotwordEntry::new(*t)).collect(),
        )
    }

    #[test]
    fn crud_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = TableStore::open(dir.path()).unwrap();

        store.save(&table("DOTA选手", &["GPK", "Ame"])).unwrap();
        store.save(&table("英雄名", &["帕克"])).unwrap();

        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "DOTA选手");

        let loaded = store.load("DOTA选手").unwrap();
        assert_eq!(loaded.entries.len(), 2);

        // Update in place.
        let mut updated = loaded;
        updated.entries.push(HotwordEntry::new("XinQ"));
        store.save(&updated).unwrap();
        assert_eq!(store.load("DOTA选手").unwrap().entries.len(), 3);

        store.delete("英雄名").unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
        assert!(matches!(
            store.load("英雄名"),
            Err(HotwordError::NotFound(_))
        ));
        assert!(matches!(
            store.delete("英雄名"),
            Err(HotwordError::NotFound(_))
        ));
    }

    #[test]
    fn merge_saves_combined_table() {
        let dir = tempfile::tempdir().unwrap();
        let store = TableStore::open(dir.path()).unwrap();
        store.save(&table("a", &["GPK", "帕克"])).unwrap();
        store.save(&table("b", &["帕克", "Ame"])).unwrap();

        let merged = store
            .merge(&["a".to_string(), "b".to_string()], "全部")
            .unwrap();
        assert_eq!(merged.entries.len(), 3);
        assert_eq!(store.load("全部").unwrap().entries.len(), 3);
        // Sources untouched.
        assert_eq!(store.load("a").unwrap().entries.len(), 2);

        assert!(store.merge(&[], "x").is_err());
        assert!(store
            .merge(&["missing".to_string()], "y")
            .is_err());
    }

    #[test]
    fn bad_names_rejected_and_cjk_kept() {
        let dir = tempfile::tempdir().unwrap();
        let store = TableStore::open(dir.path()).unwrap();
        assert!(store.save(&table("///", &["x"])).is_err());
        store.save(&table("日语 梗/表", &["草"])).unwrap();
        // Slash sanitized, CJK preserved, still loadable by original name.
        assert_eq!(store.load("日语 梗/表").unwrap().entries.len(), 1);
    }

    #[test]
    fn corrupt_files_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let store = TableStore::open(dir.path()).unwrap();
        store.save(&table("good", &["x"])).unwrap();
        std::fs::write(dir.path().join("bad.json"), "{broken").unwrap();
        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "good");
    }
}
