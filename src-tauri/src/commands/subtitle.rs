//! Live subtitle command: pull a live stream's audio through ffmpeg,
//! run streaming ASR (+ optional translation), and emit segments to the
//! frontend in real time.

use crate::state::AppState;
use std::process::Stdio;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::io::AsyncReadExt;
use vtb_asr::streaming::{StreamingAsr, StreamingConfig};
use vtb_recorder::auto::{BiliResolver, StreamResolver};
use vtb_recorder::stream::{qn, StreamApi};
use vtb_translate::{StreamerProfile, TranslateConfig, TranslatePipeline};

pub const EVENT_SUBTITLE: &str = "subtitle://segment";

#[derive(serde::Deserialize)]
pub struct LiveSubtitleOptions {
    #[serde(default)]
    pub room_id: u64,
    #[serde(default)]
    pub source_url: Option<String>,
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
    pub llm_base_url: Option<String>,
    #[serde(default)]
    pub profile_path: Option<String>,
    /// Hotword table names applied to ASR + translation.
    #[serde(default)]
    pub hotword_tables: Vec<String>,
}

#[derive(serde::Serialize, Clone)]
struct SubtitlePayload {
    room_key: String,
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
) -> Result<String, String> {
    #[cfg(not(feature = "whisper"))]
    {
        let _ = (&app, &state, &options);
        return Err("built without whisper support".into());
    }

    #[cfg(feature = "whisper")]
    {
        let room_id = options.room_id;
        let http = reqwest::Client::builder()
            .user_agent("Mozilla/5.0")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;
        let (room_key, stream) = if let Some(url) = &options.source_url {
            let info = super::youtube::request(
                &app,
                "info",
                serde_json::json!({"input":url,"fresh":true}),
            )
            .await?;
            if info["live"] != true {
                return Err("该频道当前没有直播".into());
            }
            let video = info["video_id"].as_str().ok_or("无法确定正在直播的视频")?;
            let url = format!("https://www.youtube.com/watch?v={video}");
            let stream = vtb_recorder::platform::platform_for(&url, http, qn::FLUENT)
                .resolve(&url)
                .await
                .map_err(|e| e.to_string())?;
            (format!("youtube:{video}"), stream)
        } else {
            if room_id == 0 {
                return Err("请输入 Bilibili 房间号或 YouTube 链接".into());
            }
            let resolver = BiliResolver::new(StreamApi::new(http), qn::FLUENT);
            (
                format!("bilibili:{room_id}"),
                resolver.resolve(room_id).await.map_err(|e| e.to_string())?,
            )
        };
        if state
            .subtitles
            .lock()
            .unwrap()
            .get(&room_key)
            .is_some_and(|h| !h.is_finished())
        {
            return Err("该直播的字幕任务已运行".into());
        }

        // ASR engine.
        let lang = match options.asr_lang.as_deref() {
            None | Some("auto") => None,
            Some(l) => Some(l.to_string()),
        };
        let (hotword_prompt, hotword_glossary) =
            super::hotwords::load_for_processing(&app, &options.hotword_tables);
        let mut engine_raw =
            vtb_asr::engine::WhisperEngine::new(std::path::Path::new(&options.model_path), lang)
                .map_err(|e| e.to_string())?;
        if let Some(p) = &hotword_prompt {
            engine_raw = engine_raw.with_initial_prompt(p.clone());
        }
        let engine = Arc::new(engine_raw);

        // Optional translator.
        let translator = if options.translate {
            let backend = super::pipeline::build_backend_pub(
                &app,
                options.llm_provider.clone(),
                options.llm_api_key.clone(),
                options.llm_model.clone(),
                options.llm_base_url.clone(),
            )?;
            let mut profile = match &options.profile_path {
                Some(p) => {
                    StreamerProfile::load(std::path::Path::new(p)).map_err(|e| e.to_string())?
                }
                None => StreamerProfile::default(),
            };
            super::hotwords::append_glossary(&mut profile, hotword_glossary.clone());
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
        let audio_url = stream
            .audio
            .as_ref()
            .map(|audio| audio.url.as_str())
            .unwrap_or(&stream.url);
        let audio_headers = stream
            .audio
            .as_ref()
            .map(|audio| &audio.headers)
            .unwrap_or(&stream.headers);
        for (k, v) in audio_headers {
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
        cmd.args(["-i", audio_url])
            .args(["-vn", "-ac", "1", "-ar", "16000", "-f", "s16le", "-"])
            .stdout(Stdio::piped())
            .stdin(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let mut child = cmd.spawn().map_err(|e| format!("spawn ffmpeg: {e}"))?;
        let mut stdout = child.stdout.take().ok_or("no ffmpeg stdout")?;

        let app2 = app.clone();
        let publisher = state.overlay_publisher.clone();
        let task_key = room_key.clone();
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
                                Some(t) if t.should_translate(&seg) => t
                                    .translate_segment(&seg)
                                    .await
                                    .ok()
                                    .map(|x| x.translated_text),
                                _ => None,
                            };
                            // Mirror to the OBS subtitle bar.
                            publisher.publish(vtb_overlay::OverlayMessage::Subtitle {
                                room_key: Some(task_key.clone()),
                                text: seg.text.clone(),
                                translated: translated.clone(),
                                lang: seg.lang.clone(),
                            });
                            let _ = app2.emit(
                                EVENT_SUBTITLE,
                                SubtitlePayload {
                                    room_key: task_key.clone(),
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
                publisher.publish(vtb_overlay::OverlayMessage::Subtitle {
                    room_key: Some(task_key.clone()),
                    text: seg.text.clone(),
                    translated: None,
                    lang: seg.lang.clone(),
                });
                let _ = app2.emit(
                    EVENT_SUBTITLE,
                    SubtitlePayload {
                        room_key: task_key.clone(),
                        room_id,
                        start_ms: seg.start_ms,
                        end_ms: seg.end_ms,
                        text: seg.text,
                        lang: seg.lang,
                        translated: None,
                    },
                );
            }
            publisher.publish(vtb_overlay::OverlayMessage::SubtitleClearSource {
                room_key: task_key.clone(),
            });
            let _ = app2.emit("subtitle://ended", &task_key);
        });

        let mut subs = state.subtitles.lock().unwrap();
        if subs.get(&room_key).is_some_and(|h| !h.is_finished()) {
            task.abort();
            return Err("该直播的字幕任务已运行".into());
        }
        subs.insert(room_key.clone(), task);
        Ok(room_key)
    }
}

#[tauri::command]
pub async fn live_subtitle_stop(
    app: AppHandle,
    state: State<'_, AppState>,
    room_id: Option<u64>,
    source: Option<String>,
) -> Result<(), String> {
    let key = source.unwrap_or_else(|| format!("bilibili:{}", room_id.unwrap_or(0)));
    if let Some(task) = state.subtitles.lock().unwrap().remove(&key) {
        task.abort();
        state
            .overlay_publisher
            .publish(vtb_overlay::OverlayMessage::SubtitleClearSource {
                room_key: key.clone(),
            });
        let _ = app.emit("subtitle://ended", &key);
    }
    Ok(())
}

#[tauri::command]
pub fn live_subtitle_status(state: State<'_, AppState>) -> Vec<String> {
    state
        .subtitles
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, h)| !h.is_finished())
        .map(|(k, _)| k.clone())
        .collect()
}
