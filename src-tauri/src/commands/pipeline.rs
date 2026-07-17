//! Offline processing command: run the full pipeline on a recording.

use crate::state::AppState;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;
use vtb_pipeline::{JobConfig, OfflineJob};
use vtb_translate::{OpenAiCompatBackend, AnthropicBackend, LlmBackend, StreamerProfile};

pub const EVENT_JOB_PROGRESS: &str = "job://progress";
pub const EVENT_JOB_DONE: &str = "job://done";

#[derive(serde::Deserialize)]
pub struct OfflineOptions {
    pub job_id: String,
    pub input: String,
    pub output_dir: String,
    #[serde(default)]
    pub danmaku_log: Option<String>,
    /// RFC3339 session start (for danmaku offset alignment).
    #[serde(default)]
    pub session_start: Option<String>,
    #[serde(default)]
    pub translate: bool,
    #[serde(default = "default_true")]
    pub highlights: bool,
    #[serde(default = "default_lang")]
    pub target_lang: String,
    /// Path to a ggml whisper model.
    pub model_path: String,
    /// Whisper language hint ("auto" for detection).
    #[serde(default)]
    pub asr_lang: Option<String>,
    /// LLM backend: "anthropic" | "openai"
    #[serde(default)]
    pub llm_provider: Option<String>,
    #[serde(default)]
    pub llm_api_key: Option<String>,
    #[serde(default)]
    pub llm_model: Option<String>,
    #[serde(default)]
    pub llm_base_url: Option<String>,
    /// Streamer profile JSON path for personalized translation.
    #[serde(default)]
    pub profile_path: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_lang() -> String {
    "zh".into()
}

#[derive(serde::Serialize, Clone)]
struct ProgressPayload {
    job_id: String,
    stage: String,
    fraction: Option<f64>,
    message: String,
}

#[derive(serde::Serialize, Clone)]
struct DonePayload {
    job_id: String,
    ok: bool,
    message: String,
    transcript_segments: usize,
    translations: usize,
    highlights: usize,
    clips: usize,
}

/// Shared LLM backend construction (also used by the live subtitle command).
pub fn build_backend_pub(
    provider: Option<&str>,
    api_key: Option<String>,
    model: Option<String>,
    base_url: Option<String>,
) -> Result<Arc<dyn LlmBackend>, String> {
    let key = api_key
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
        .ok_or("no LLM API key provided")?;
    match provider.unwrap_or("anthropic") {
        "openai" => Ok(Arc::new(OpenAiCompatBackend::new(
            base_url.unwrap_or_else(|| "https://api.openai.com/v1".into()),
            key,
            model.unwrap_or_else(|| "gpt-4o-mini".into()),
        ))),
        _ => {
            let mut b = AnthropicBackend::new(
                key,
                model.unwrap_or_else(|| "claude-haiku-4-5".into()),
            );
            if let Some(url) = base_url {
                b = b.with_base_url(url);
            }
            Ok(Arc::new(b))
        }
    }
}

fn build_backend(o: &OfflineOptions) -> Result<Arc<dyn LlmBackend>, String> {
    build_backend_pub(
        o.llm_provider.as_deref(),
        o.llm_api_key.clone(),
        o.llm_model.clone(),
        o.llm_base_url.clone(),
    )
}

#[tauri::command]
pub async fn offline_process(
    app: AppHandle,
    state: State<'_, AppState>,
    options: OfflineOptions,
) -> Result<(), String> {
    #[cfg(not(feature = "whisper"))]
    {
        let _ = (&app, &state, &options);
        return Err("built without whisper support".into());
    }

    #[cfg(feature = "whisper")]
    {
        let job_id = options.job_id.clone();
        {
            let jobs = state.jobs.lock().unwrap();
            if jobs.get(&job_id).map(|h| !h.is_finished()).unwrap_or(false) {
                return Err(format!("job {job_id} already running"));
            }
        }

        let mut cfg = JobConfig::new(&options.input, &options.output_dir);
        cfg.translate = options.translate;
        cfg.highlights = options.highlights;
        cfg.target_lang = options.target_lang.clone();
        cfg.danmaku_log = options.danmaku_log.as_ref().map(PathBuf::from);
        cfg.session_start = options
            .session_start
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&chrono::Utc));

        let lang = match options.asr_lang.as_deref() {
            None | Some("auto") => None,
            Some(l) => Some(l.to_string()),
        };
        let engine = Arc::new(
            vtb_asr::engine::WhisperEngine::new(
                std::path::Path::new(&options.model_path),
                lang,
            )
            .map_err(|e| e.to_string())?,
        );

        let mut job = OfflineJob::new(cfg, engine);
        if options.translate {
            let backend = build_backend(&options)?;
            let profile = match &options.profile_path {
                Some(p) => StreamerProfile::load(std::path::Path::new(p))
                    .map_err(|e| e.to_string())?,
                None => StreamerProfile::default(),
            };
            job = job.with_translator(backend, profile);
        }

        let (ptx, mut prx) = mpsc::channel(64);
        job = job.with_progress(ptx);

        // Progress pump.
        let app2 = app.clone();
        let jid = job_id.clone();
        tokio::spawn(async move {
            while let Some(p) = prx.recv().await {
                let _ = app2.emit(
                    EVENT_JOB_PROGRESS,
                    ProgressPayload {
                        job_id: jid.clone(),
                        stage: format!("{:?}", p.stage),
                        fraction: p.fraction,
                        message: p.message,
                    },
                );
            }
        });

        let app3 = app.clone();
        let jid2 = job_id.clone();
        let task = tokio::spawn(async move {
            let result = job.run().await;
            let payload = match result {
                Ok(out) => DonePayload {
                    job_id: jid2.clone(),
                    ok: true,
                    message: "done".into(),
                    transcript_segments: out.transcript.len(),
                    translations: out.translations.len(),
                    highlights: out.highlights.len(),
                    clips: out.clip_files.len(),
                },
                Err(e) => DonePayload {
                    job_id: jid2.clone(),
                    ok: false,
                    message: e.to_string(),
                    transcript_segments: 0,
                    translations: 0,
                    highlights: 0,
                    clips: 0,
                },
            };
            let _ = app3.emit(EVENT_JOB_DONE, &payload);
        });

        state.jobs.lock().unwrap().insert(job_id, task);
        Ok(())
    }
}

#[tauri::command]
pub async fn offline_cancel(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<(), String> {
    if let Some(task) = state.jobs.lock().unwrap().remove(&job_id) {
        task.abort();
        Ok(())
    } else {
        Err(format!("job {job_id} not found"))
    }
}
