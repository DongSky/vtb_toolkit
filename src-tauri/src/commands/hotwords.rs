//! 热词表 management commands: create from free-form text (LLM-normalized
//! with rule fallback), list/get/update/merge/delete, and apply to
//! ASR/translation.

use crate::state::AppState;
use tauri::{AppHandle, Manager, State};
use vtb_hotwords::{HotwordTable, TableStore};

pub fn hotwords_store(app: &AppHandle) -> Result<TableStore, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("hotwords");
    TableStore::open(dir).map_err(|e| e.to_string())
}

/// Adapter: vtb-translate LLM backend → vtb-hotwords TextCompleter.
struct BackendCompleter(std::sync::Arc<dyn vtb_translate::LlmBackend>);

#[async_trait::async_trait]
impl vtb_hotwords::TextCompleter for BackendCompleter {
    async fn complete(&self, system: &str, user: &str) -> Result<String, String> {
        self.0
            .complete(system, user)
            .await
            .map_err(|e| e.to_string())
    }
}

#[derive(serde::Serialize)]
pub struct TableSummary {
    pub name: String,
    pub description: String,
    pub entries: usize,
    pub updated_at: String,
}

#[derive(serde::Serialize)]
pub struct SaveRawResult {
    pub table: HotwordTable,
    /// Whether the LLM normalizer was used (false = rule-parser fallback).
    pub used_llm: bool,
}

#[tauri::command]
pub async fn hotwords_list(app: AppHandle) -> Result<Vec<TableSummary>, String> {
    let store = hotwords_store(&app)?;
    Ok(store
        .list()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|t| TableSummary {
            name: t.name,
            description: t.description,
            entries: t.entries.len(),
            updated_at: t.updated_at.to_rfc3339(),
        })
        .collect())
}

#[tauri::command]
pub async fn hotwords_get(app: AppHandle, name: String) -> Result<HotwordTable, String> {
    hotwords_store(&app)?.load(&name).map_err(|e| e.to_string())
}

/// Create/replace a table from arbitrary free-form text. `use_llm` asks the
/// configured LLM to normalize; failure or `false` falls back to rules.
#[tauri::command]
pub async fn hotwords_save_raw(
    app: AppHandle,
    name: String,
    raw: String,
    description: Option<String>,
    use_llm: bool,
) -> Result<SaveRawResult, String> {
    let (entries, used_llm) = if use_llm {
        match super::pipeline::build_backend_pub(&app, None, None, None, None) {
            Ok(backend) => {
                let completer = BackendCompleter(backend);
                vtb_hotwords::normalize_with_llm(&completer, &raw).await
            }
            Err(e) => {
                tracing::warn!("LLM 不可用,回退规则解析: {e}");
                (vtb_hotwords::parse_rules(&raw), false)
            }
        }
    } else {
        (vtb_hotwords::parse_rules(&raw), false)
    };
    if entries.is_empty() {
        return Err("没有解析出任何词条,请检查输入".into());
    }
    let mut table = HotwordTable::new(&name, entries);
    table.description = description.unwrap_or_default();
    hotwords_store(&app)?
        .save(&table)
        .map_err(|e| e.to_string())?;
    Ok(SaveRawResult { table, used_llm })
}

/// Overwrite a table with edited entries (structured update).
#[tauri::command]
pub async fn hotwords_update(
    app: AppHandle,
    table: HotwordTable,
) -> Result<HotwordTable, String> {
    let mut t = HotwordTable::new(&table.name, table.entries);
    t.description = table.description;
    hotwords_store(&app)?.save(&t).map_err(|e| e.to_string())?;
    Ok(t)
}

#[tauri::command]
pub async fn hotwords_delete(app: AppHandle, name: String) -> Result<(), String> {
    hotwords_store(&app)?
        .delete(&name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn hotwords_merge(
    app: AppHandle,
    names: Vec<String>,
    new_name: String,
) -> Result<HotwordTable, String> {
    hotwords_store(&app)?
        .merge(&names, &new_name)
        .map_err(|e| e.to_string())
}

/// Load selected tables and build the ASR prompt + translation glossary
/// additions used by the processing commands.
pub fn load_for_processing(
    app: &AppHandle,
    names: &[String],
) -> (Option<String>, Vec<vtb_translate::GlossaryEntry>) {
    let Ok(store) = hotwords_store(app) else {
        return (None, vec![]);
    };
    let tables: Vec<HotwordTable> = names
        .iter()
        .filter_map(|n| store.load(n).ok())
        .collect();
    if tables.is_empty() {
        return (None, vec![]);
    }
    let refs: Vec<&HotwordTable> = tables.iter().collect();
    let prompt = vtb_hotwords::asr_initial_prompt(&refs, 800);
    let glossary = vtb_hotwords::translation_pairs(&refs)
        .into_iter()
        .map(|(source, target, note)| vtb_translate::GlossaryEntry {
            source,
            target,
            note,
        })
        .collect();
    (
        (!prompt.is_empty()).then_some(prompt),
        glossary,
    )
}

/// Trait shared by ASR-using commands to accept hotword table names.
pub fn append_glossary(
    profile: &mut vtb_translate::StreamerProfile,
    extra: Vec<vtb_translate::GlossaryEntry>,
) {
    for e in extra {
        if !profile
            .glossary
            .entries
            .iter()
            .any(|x| x.source.eq_ignore_ascii_case(&e.source))
        {
            profile.glossary.entries.push(e);
        }
    }
}
