//! Live subtitle command: pull a live stream's audio through ffmpeg,
//! run streaming ASR (+ optional translation), and emit segments to the
//! frontend in real time.

use crate::state::AppState;
use std::process::Stdio;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use vtb_asr::streaming::{StreamingAsr, StreamingConfig};
use vtb_recorder::auto::{BiliResolver, StreamResolver};
use vtb_recorder::stream::{qn, StreamApi};
use vtb_translate::{StreamerProfile, TranslateConfig, TranslatePipeline};

pub const EVENT_SUBTITLE: &str = "subtitle://segment";

#[derive(serde::Deserialize)]
pub struct LiveSubtitleOptions {
    pub room_id: u64,
    pub model_path: String,
    #[serde(default)]
    pub asr_lang: Option<String>,
    #[serde(default)]
    pub translate: bool,
    #[serde(default)]
    pub target_lang: Option<String>,
    #[serde(default)]
    pub llm_provider: Option<String>,
    #[serde(default)]
    pub llm_api_key: Option<String>,
    #[serde(default)]
    pub llm_model: Option<String>,
    #[serde(default)]
    pub profile_path: Option<String>,
}

#[derive(serde::Serialize, Clone)]
struct SubtitlePayload {
    room_id: u64,
    start_ms: u64,
    end_ms: u64,
    text: String,
    lang: Option<String>,
    translated: Option<String>,
}

#[tauri::command]
pub async fn live_subtitle_start(
    app: AppHandle,
    state: State<'_, AppState>,
    options: LiveSubtitleOptions,
) -> Result<(), String> {
    #[cfg(not(feature = "whisper"))]
    {
        let _ = (&app, &state, &options);
        return Err("built without whisper support".into());
    }

    #[cfg(feature = "whisper")]
    {
        let room_id = options.room_id;
        {
            let subs = state.subtitles.lock().unwrap();
            if subs.get(&room_id).map(|h| !h.is_finished()).unwrap_or(false) {
                return Err(format!("room {room_id} subtitle already running"));
            }
        }

        // Resolve the stream URL.
        let http = reqwest::Client::builder()
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
            )
            .build()
            .map_err(|e| e.to_string())?;
        let resolver = BiliResolver::new(StreamApi::new(http), qn::FLUENT);
        let stream = resolver.resolve(room_id).await.map_err(|e| e.to_string())?;

        // ASR engine.
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

        // Optional translator.
        let translator = if options.translate {
            let backend = super::pipeline::build_backend_pub(
                options.llm_provider.as_deref(),
                options.llm_api_key.clone(),
                options.llm_model.clone(),
                None,
            )?;
            let profile = match &options.profile_path {
                Some(p) => StreamerProfile::load(std::path::Path::new(p))
                    .map_err(|e| e.to_string())?,
                None => StreamerProfile::default(),
            };
            Some(TranslatePipeline::new(
                backend,
                profile,
                TranslateConfig {
                    target_lang: options.target_lang.clone().unwrap_or_else(|| "zh".into()),
                    ..Default::default()
                },
            ))
        } else {
            None
        };

        // ffmpeg: stream URL → 16k mono s16le on stdout.
        let mut headers = String::new();
        for (k, v) in &stream.headers {
            headers.push_str(&format!("{k}: {v}\r\n"));
        }
        let mut cmd = tokio::process::Command::new("ffmpeg");
        cmd.args(["-hide_banner", "-loglevel", "error"]);
        if let Some(ua) = &stream.user_agent {
            cmd.args(["-user_agent", ua]);
        }
        if !headers.is_empty() {
            cmd.args(["-headers", &headers]);
        }
        cmd.args(["-i", &stream.url])
            .args(["-vn", "-ac", "1", "-ar", "16000", "-f", "s16le", "-"])
            .stdout(Stdio::piped())
            .stdin(Stdio::null())
            .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| format!("spawn ffmpeg: {e}"))?;
        let mut stdout = child.stdout.take().ok_or("no ffmpeg stdout")?;

        let app2 = app.clone();
        let task = tokio::spawn(async move {
            let _child = child; // keep alive; kill_on_drop stops ffmpeg on abort
            let mut asr = StreamingAsr::new(engine, StreamingConfig::default());
            let mut translator = translator;
            // 0.5 s of s16le @16k = 16000 bytes.
            let mut buf = vec![0u8; 16000];
            loop {
                match stdout.read_exact(&mut buf).await {
                    Ok(_) => {
                        let pcm: Vec<f32> = buf
                            .chunks_exact(2)
                            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32767.0)
                            .collect();
                        for seg in asr.feed(&pcm).await {
                            let translated = match &mut translator {
                                Some(t) if t.should_translate(&seg) => {
                                    t.translate_segment(&seg).await.ok().map(|x| x.translated_text)
                                }
                                _ => None,
                            };
                            let _ = app2.emit(
                                EVENT_SUBTITLE,
                                SubtitlePayload {
                                    room_id,
                                    start_ms: seg.start_ms,
                                    end_ms: seg.end_ms,
                                    text: seg.text,
                                    lang: seg.lang,
                                    translated,
                                },
                            );
                        }
                    }
                    Err(_) => break, // stream ended
                }
            }
            for seg in asr.finish().await {
                let _ = app2.emit(
                    EVENT_SUBTITLE,
                    SubtitlePayload {
                        room_id,
                        start_ms: seg.start_ms,
                        end_ms: seg.end_ms,
                        text: seg.text,
                        lang: seg.lang,
                        translated: None,
                    },
                );
            }
        });

        state.subtitles.lock().unwrap().insert(room_id, task);
        Ok(())
    }
}

#[tauri::command]
pub async fn live_subtitle_stop(
    state: State<'_, AppState>,
    room_id: u64,
) -> Result<(), String> {
    if let Some(task) = state.subtitles.lock().unwrap().remove(&room_id) {
        task.abort();
        Ok(())
    } else {
        Err(format!("room {room_id} subtitle not running"))
    }
}
