//! Offline job orchestration: recording file → transcript → translation →
//! subtitles → highlights → clips, with per-stage progress reporting.

use crate::danmaku_log;
use crate::energy::rms_series;
use crate::error::{PipelineError, Result};
use crate::subtitle;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use vtb_asr::engine::AsrEngine;
use vtb_asr::streaming::{transcribe_buffer, StreamingConfig};
use vtb_common::{Highlight, TranscriptSegment, TranslatedSegment};
use vtb_highlight::fusion::{detect_highlights, FusionConfig, SignalSet};
use vtb_highlight::signals::{danmaku_density, gift_value, keyword_score, audio_energy_scores};
use vtb_translate::{LlmBackend, StreamerProfile, TranslateConfig, TranslatePipeline};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    ExtractAudio,
    Transcribe,
    Translate,
    ExportSubtitles,
    DetectHighlights,
    CutClips,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProgress {
    pub stage: Stage,
    /// 0.0..=1.0 within the stage where known.
    pub fraction: Option<f64>,
    pub message: String,
}

#[derive(Clone)]
pub struct JobConfig {
    /// Input video/recording.
    pub input: PathBuf,
    /// Output directory (subtitles, clips, transcript JSON).
    pub output_dir: PathBuf,
    /// Optional danmaku JSONL recorded during the stream.
    pub danmaku_log: Option<PathBuf>,
    /// Session start (needed to align danmaku offsets); defaults to file
    /// mtime minus duration heuristics — better supplied by the recorder.
    pub session_start: Option<DateTime<Utc>>,
    /// Translate transcripts?
    pub translate: bool,
    /// Detect highlights and cut clips?
    pub highlights: bool,
    pub target_lang: String,
    pub ffmpeg: PathBuf,
    pub fusion: FusionConfig,
    pub streaming: StreamingConfig,
}

impl JobConfig {
    pub fn new(input: impl Into<PathBuf>, output_dir: impl Into<PathBuf>) -> Self {
        Self {
            input: input.into(),
            output_dir: output_dir.into(),
            danmaku_log: None,
            session_start: None,
            translate: false,
            highlights: true,
            target_lang: "zh".into(),
            ffmpeg: PathBuf::from("ffmpeg"),
            fusion: FusionConfig::default(),
            streaming: StreamingConfig::default(),
        }
    }
}

/// Results of a completed job.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobOutput {
    pub transcript: Vec<TranscriptSegment>,
    pub translations: Vec<TranslatedSegment>,
    pub highlights: Vec<Highlight>,
    pub subtitle_files: Vec<PathBuf>,
    pub clip_files: Vec<PathBuf>,
}

pub struct OfflineJob {
    config: JobConfig,
    asr: Arc<dyn AsrEngine>,
    translator: Option<Arc<dyn LlmBackend>>,
    profile: StreamerProfile,
    progress: Option<mpsc::Sender<JobProgress>>,
}

impl OfflineJob {
    pub fn new(config: JobConfig, asr: Arc<dyn AsrEngine>) -> Self {
        Self {
            config,
            asr,
            translator: None,
            profile: StreamerProfile::default(),
            progress: None,
        }
    }

    pub fn with_translator(
        mut self,
        backend: Arc<dyn LlmBackend>,
        profile: StreamerProfile,
    ) -> Self {
        self.translator = Some(backend);
        self.profile = profile;
        self
    }

    pub fn with_progress(mut self, tx: mpsc::Sender<JobProgress>) -> Self {
        self.progress = Some(tx);
        self
    }

    async fn report(&self, stage: Stage, fraction: Option<f64>, message: impl Into<String>) {
        if let Some(tx) = &self.progress {
            let _ = tx
                .send(JobProgress {
                    stage,
                    fraction,
                    message: message.into(),
                })
                .await;
        }
    }

    /// Run the job to completion, consuming it. Consuming `self` guarantees
    /// the progress sender is dropped when the job ends, so a receiver loop
    /// `while let Some(p) = rx.recv().await` terminates instead of
    /// deadlocking (regression found in live testing).
    pub async fn run(self) -> Result<JobOutput> {
        std::fs::create_dir_all(&self.config.output_dir)?;
        let mut out = JobOutput::default();

        // 1. Extract audio.
        self.report(Stage::ExtractAudio, None, "提取音频").await;
        let pcm =
            vtb_asr::audio::extract_audio_ffmpeg(&self.config.ffmpeg, &self.config.input)
                .await?;
        let total_ms = (pcm.len() as u64 * 1000) / vtb_asr::SAMPLE_RATE as u64;

        // 2. Transcribe.
        self.report(Stage::Transcribe, None, "语音识别").await;
        out.transcript =
            transcribe_buffer(self.asr.clone(), &pcm, self.config.streaming.clone()).await;
        let transcript_json = self.config.output_dir.join("transcript.json");
        std::fs::write(&transcript_json, serde_json::to_string_pretty(&out.transcript)?)?;

        // 3. Translate (optional).
        if self.config.translate {
            let backend = self.translator.clone().ok_or_else(|| {
                PipelineError::Config("translate=true but no LLM backend set".into())
            })?;
            let mut pipeline = TranslatePipeline::new(
                backend,
                self.profile.clone(),
                TranslateConfig {
                    target_lang: self.config.target_lang.clone(),
                    ..Default::default()
                },
            );
            let n = out.transcript.len().max(1);
            for (i, seg) in out.transcript.clone().iter().enumerate() {
                self.report(
                    Stage::Translate,
                    Some(i as f64 / n as f64),
                    format!("翻译 {}/{}", i + 1, n),
                )
                .await;
                if pipeline.should_translate(seg) {
                    match pipeline.translate_segment(seg).await {
                        Ok(t) => out.translations.push(t),
                        Err(e) => tracing::warn!("translate segment failed: {e}"),
                    }
                }
            }
        }

        // 4. Subtitles.
        self.report(Stage::ExportSubtitles, None, "导出字幕").await;
        let srt = self.config.output_dir.join("subtitles.srt");
        std::fs::write(&srt, subtitle::to_srt(&out.transcript))?;
        out.subtitle_files.push(srt);
        if !out.translations.is_empty() {
            let bi_srt = self.config.output_dir.join("subtitles.bilingual.srt");
            std::fs::write(&bi_srt, subtitle::to_srt_bilingual(&out.translations))?;
            out.subtitle_files.push(bi_srt);
            let ass = self.config.output_dir.join("subtitles.bilingual.ass");
            std::fs::write(&ass, subtitle::to_ass_bilingual(&out.translations))?;
            out.subtitle_files.push(ass);
        }

        // 5. Highlights.
        if self.config.highlights {
            self.report(Stage::DetectHighlights, None, "检测高能片段").await;
            let window_ms = 10_000u64;
            let audio = audio_energy_scores(&rms_series(&pcm, 1000), window_ms, total_ms);

            let mut signals = SignalSet {
                audio: Some(audio),
                ..Default::default()
            };

            // Danmaku signals when a log + session start are available.
            let entries;
            if let (Some(log), Some(start)) =
                (&self.config.danmaku_log, self.config.session_start)
            {
                entries = danmaku_log::read_log(log)?;
                let offsets = danmaku_log::to_offsets(&entries, start);
                signals.density = Some(danmaku_density(&offsets, window_ms, total_ms));
                signals.keyword = Some(keyword_score(&offsets, window_ms, total_ms));
                signals.gift = Some(gift_value(&offsets, window_ms, total_ms));
            }

            out.highlights = detect_highlights(&signals, &self.config.fusion, total_ms);
            let hl_json = self.config.output_dir.join("highlights.json");
            std::fs::write(&hl_json, serde_json::to_string_pretty(&out.highlights)?)?;

            // 6. Cut clips.
            let clip_opts = vtb_highlight::clip::ClipOptions {
                input: self.config.input.clone(),
                output_dir: self.config.output_dir.join("clips"),
                reencode: false,
            };
            let n = out.highlights.len().max(1);
            for (i, h) in out.highlights.iter().enumerate() {
                self.report(
                    Stage::CutClips,
                    Some(i as f64 / n as f64),
                    format!("切片 {}/{}", i + 1, n),
                )
                .await;
                match vtb_highlight::clip::cut_clip(&self.config.ffmpeg, &clip_opts, h).await
                {
                    Ok(path) => out.clip_files.push(path),
                    Err(e) => tracing::warn!("clip cut failed: {e}"),
                }
            }
        }

        self.report(Stage::Done, Some(1.0), "完成").await;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use vtb_asr::engine::mock::MockEngine;
    use vtb_asr::engine::Recognition;

    fn ffmpeg_on_path() -> bool {
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).any(|d| d.join("ffmpeg").exists()))
            .unwrap_or(false)
    }

    /// Generate a 6 s test video with alternating loud tone bursts so the
    /// VAD finds utterances and highlight audio-energy has variance.
    async fn generate_test_video(path: &Path) {
        let st = tokio::process::Command::new("ffmpeg")
            .args([
                "-y", "-hide_banner", "-loglevel", "error",
                "-f", "lavfi", "-i", "testsrc=duration=6:size=320x240:rate=10",
                "-f", "lavfi", "-i",
                "sine=frequency=440:duration=6,volume='if(lt(mod(t,2),1),1.0,0.0)':eval=frame",
                "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac",
                "-shortest",
            ])
            .arg(path)
            .status()
            .await
            .unwrap();
        assert!(st.success());
    }

    #[tokio::test]
    async fn full_offline_job_with_mock_asr() {
        if !ffmpeg_on_path() {
            eprintln!("ffmpeg not found; skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("rec.mp4");
        generate_test_video(&video).await;

        let engine = Arc::new(MockEngine::new(vec![
            Recognition { text: "第一段".into(), lang: Some("zh".into()) },
            Recognition { text: "第二段".into(), lang: Some("zh".into()) },
            Recognition { text: "第三段".into(), lang: Some("zh".into()) },
        ]));

        let mut cfg = JobConfig::new(&video, dir.path().join("out"));
        // Make the fusion permissive so short test media yields highlights.
        cfg.fusion.threshold = 0.5;
        cfg.fusion.min_len_ms = 0;
        cfg.fusion.pad_ms = 0;

        let (tx, mut rx) = mpsc::channel(64);
        let job = OfflineJob::new(cfg, engine).with_progress(tx);
        let out = job.run().await.unwrap();

        // Transcript produced and persisted.
        assert!(!out.transcript.is_empty(), "expected VAD utterances");
        assert!(dir.path().join("out/transcript.json").exists());
        // SRT exists and is non-empty.
        let srt = std::fs::read_to_string(dir.path().join("out/subtitles.srt")).unwrap();
        assert!(srt.contains("-->"));
        // Highlights JSON persisted.
        assert!(dir.path().join("out/highlights.json").exists());
        // Progress events observed, ending with Done.
        let mut stages = Vec::new();
        while let Ok(p) = rx.try_recv() {
            stages.push(p.stage);
        }
        assert_eq!(*stages.last().unwrap(), Stage::Done);
        assert!(stages.contains(&Stage::Transcribe));
    }

    /// Regression: `run` must consume the job so its progress sender drops
    /// on completion; a `while rx.recv()` pump then terminates. With
    /// `run(&self)` this deadlocked (job outlived run, channel never closed).
    #[tokio::test]
    async fn progress_channel_closes_after_run() {
        if !ffmpeg_on_path() {
            eprintln!("ffmpeg not found; skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("rec.mp4");
        generate_test_video(&video).await;

        let mut cfg = JobConfig::new(&video, dir.path().join("out"));
        cfg.highlights = false;
        let (tx, mut rx) = mpsc::channel(64);
        let job = OfflineJob::new(cfg, Arc::new(MockEngine::empty())).with_progress(tx);

        // Pump in a task exactly like real callers do.
        let pump = tokio::spawn(async move {
            let mut n = 0;
            while rx.recv().await.is_some() {
                n += 1;
            }
            n
        });

        job.run().await.unwrap();
        // Must complete promptly — not hang forever.
        let n = tokio::time::timeout(std::time::Duration::from_secs(5), pump)
            .await
            .expect("pump deadlocked: progress sender not dropped")
            .unwrap();
        assert!(n > 0, "expected progress events");
    }

    #[tokio::test]
    async fn job_with_danmaku_log_and_translation() {
        if !ffmpeg_on_path() {
            eprintln!("ffmpeg not found; skipping");
            return;
        }
        use crate::danmaku_log::{DanmakuLogWriter, LogEntry};
        use chrono::TimeZone;
        use vtb_common::{DanmakuMsg, LiveEvent};
        use vtb_translate::backend::mock::MockBackend;

        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("rec.mp4");
        generate_test_video(&video).await;

        // Danmaku burst at t=2..3s.
        let start = Utc.timestamp_opt(1_700_000_000, 0).single().unwrap();
        let log_path = dir.path().join("danmaku.jsonl");
        {
            let mut w = DanmakuLogWriter::create(&log_path).unwrap();
            for i in 0..20 {
                let ts = start + chrono::Duration::milliseconds(2000 + i * 40);
                w.write(&LogEntry {
                    received_at: ts,
                    event: LiveEvent::Danmaku(DanmakuMsg {
                        room_id: 1,
                        uid: i as u64,
                        username: "u".into(),
                        text: "草".into(),
                        timestamp: ts,
                        medal: None,
                        guard_level: 0,
                        is_admin: false,
                        emoticon: None,
                    }),
                })
                .unwrap();
            }
            w.flush().unwrap();
        }

        let mut cfg = JobConfig::new(&video, dir.path().join("out"));
        cfg.danmaku_log = Some(log_path);
        cfg.session_start = Some(start);
        cfg.translate = true;
        // Mock ASR reports lang "zh"; target must differ or the pipeline's
        // same-language skip (correctly) drops every segment.
        cfg.target_lang = "en".into();
        cfg.fusion.threshold = 0.8;
        cfg.fusion.min_len_ms = 0;
        cfg.fusion.pad_ms = 0;

        let engine = Arc::new(MockEngine::empty());
        let job = OfflineJob::new(cfg, engine)
            .with_translator(Arc::new(MockBackend::echo()), StreamerProfile::default());
        let out = job.run().await.unwrap();

        assert!(!out.transcript.is_empty());
        assert_eq!(out.translations.len(), out.transcript.len());
        assert!(out
            .subtitle_files
            .iter()
            .any(|p| p.to_string_lossy().contains("bilingual")));
    }

    #[tokio::test]
    async fn translate_without_backend_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = JobConfig::new(dir.path().join("missing.mp4"), dir.path());
        cfg.translate = true;
        let job = OfflineJob::new(cfg, Arc::new(MockEngine::empty()));
        // Fails at audio extraction (missing file) or config — either way Err.
        assert!(job.run().await.is_err());
    }
}
