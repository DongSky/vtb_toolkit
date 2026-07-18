//! Danmaku auto-translation (blivechat-style): a background worker that
//! translates chat lines with the configured LLM and pushes results to
//! both the in-app list and the OBS overlay, keyed by a correlation id.

use crate::state::{AppState, DanmakuTranslateHandle};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, State};
use vtb_translate::{StreamerProfile, TranslateConfig, TranslatePipeline};

pub const EVENT_DANMAKU_TRANSLATION: &str = "danmaku://translation";

static NEXT_TID: AtomicU64 = AtomicU64::new(1);

/// Allocate a translation correlation id (unique across rooms).
pub fn next_tid() -> u64 {
    NEXT_TID.fetch_add(1, Ordering::Relaxed)
}

#[derive(serde::Deserialize)]
pub struct DanmakuTranslateOptions {
    #[serde(default)]
    pub target_lang: Option<String>,
    #[serde(default)]
    pub llm_provider: Option<String>,
    #[serde(default)]
    pub llm_api_key: Option<String>,
    #[serde(default)]
    pub llm_model: Option<String>,
    #[serde(default)]
    pub llm_base_url: Option<String>,
    #[serde(default)]
    pub profile_path: Option<String>,
    /// Hotword table names whose fixed translations extend the glossary.
    #[serde(default)]
    pub hotword_tables: Vec<String>,
}

#[derive(serde::Serialize, Clone)]
struct TranslationPayload {
    tid: u64,
    translated: String,
}

#[tauri::command]
pub async fn danmaku_translate_start(
    app: AppHandle,
    state: State<'_, AppState>,
    options: DanmakuTranslateOptions,
) -> Result<(), String> {
    let target = options.target_lang.clone().unwrap_or_else(|| "ja".into());
    let backend = super::pipeline::build_backend_pub(
        &app,
        options.llm_provider.clone(),
        options.llm_api_key.clone(),
        options.llm_model.clone(),
        options.llm_base_url.clone(),
    )?;
    let mut profile = match &options.profile_path {
        Some(p) => StreamerProfile::load(std::path::Path::new(p))
            .map_err(|e| e.to_string())?,
        None => StreamerProfile::default(),
    };
    let (_, glossary) = super::hotwords::load_for_processing(&app, &options.hotword_tables);
    super::hotwords::append_glossary(&mut profile, glossary);

    let mut pipeline = TranslatePipeline::new(
        backend,
        profile,
        TranslateConfig {
            target_lang: target.clone(),
            context_lines: 6,
            min_chars: 2,
            // The pump already script-filters via should_translate_danmaku;
            // segments here have no detected lang anyway.
            skip_same_lang: false,
        },
    );

    // Bounded queue: the pump drops (rather than delays) chat lines when
    // translation can't keep up with a busy room.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(u64, String)>(64);
    let publisher = state.overlay_publisher.clone();
    let app2 = app.clone();
    let task = tokio::spawn(async move {
        while let Some((tid, text)) = rx.recv().await {
            let seg = vtb_common::TranscriptSegment {
                start_ms: 0,
                end_ms: 0,
                text,
                lang: None,
                is_final: true,
            };
            match pipeline.translate_segment(&seg).await {
                Ok(t) => {
                    let translated = t.translated_text.trim().to_string();
                    if translated.is_empty() {
                        continue;
                    }
                    publisher.publish(vtb_overlay::OverlayMessage::DanmakuTranslation {
                        tid,
                        translated: translated.clone(),
                    });
                    let _ = app2.emit(
                        EVENT_DANMAKU_TRANSLATION,
                        TranslationPayload { tid, translated },
                    );
                }
                Err(e) => tracing::warn!("danmaku translate failed: {e}"),
            }
        }
    });

    let mut guard = state.danmaku_translate.lock().unwrap();
    if let Some(old) = guard.take() {
        old.task.abort();
    }
    *guard = Some(DanmakuTranslateHandle {
        tx,
        task,
        target_lang: target,
    });
    Ok(())
}

#[tauri::command]
pub async fn danmaku_translate_stop(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(h) = state.danmaku_translate.lock().unwrap().take() {
        h.task.abort();
        Ok(())
    } else {
        Err("danmaku translation not running".into())
    }
}

/// Returns the active target language, or null when not running.
#[tauri::command]
pub async fn danmaku_translate_status(
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let guard = state.danmaku_translate.lock().unwrap();
    Ok(guard
        .as_ref()
        .filter(|h| !h.task.is_finished())
        .map(|h| h.target_lang.clone()))
}
