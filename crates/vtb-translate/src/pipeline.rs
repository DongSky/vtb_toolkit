//! The streaming translation pipeline: consumes final ASR segments,
//! maintains a rolling context window, and emits translated segments.

use crate::backend::LlmBackend;
use crate::error::Result;
use crate::glossary::StreamerProfile;
use crate::prompt::{build_system_prompt, build_user_prompt};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::mpsc;
use vtb_common::{TranscriptSegment, TranslatedSegment};

#[derive(Debug, Clone)]
pub struct TranslateConfig {
    pub target_lang: String,
    /// How many previous source lines to include as context.
    pub context_lines: usize,
    /// Skip segments shorter than this (filters "うん", "あー" filler).
    pub min_chars: usize,
}

impl Default for TranslateConfig {
    fn default() -> Self {
        Self {
            target_lang: "zh".into(),
            context_lines: 4,
            min_chars: 2,
        }
    }
}

pub struct TranslatePipeline {
    backend: Arc<dyn LlmBackend>,
    profile: StreamerProfile,
    config: TranslateConfig,
    history: VecDeque<String>,
    system_prompt: String,
}

impl TranslatePipeline {
    pub fn new(
        backend: Arc<dyn LlmBackend>,
        profile: StreamerProfile,
        config: TranslateConfig,
    ) -> Self {
        let system_prompt = build_system_prompt(&profile, &config.target_lang);
        Self {
            backend,
            profile,
            config,
            history: VecDeque::new(),
            system_prompt,
        }
    }

    /// Update the profile (e.g. user edited the glossary mid-stream).
    pub fn set_profile(&mut self, profile: StreamerProfile) {
        self.profile = profile;
        self.system_prompt =
            build_system_prompt(&self.profile, &self.config.target_lang);
    }

    /// Whether a segment should be translated at all.
    pub fn should_translate(&self, seg: &TranscriptSegment) -> bool {
        seg.is_final && seg.text.trim().chars().count() >= self.config.min_chars
    }

    /// Translate one final segment, updating rolling context.
    pub async fn translate_segment(
        &mut self,
        seg: &TranscriptSegment,
    ) -> Result<TranslatedSegment> {
        let history: Vec<String> = self.history.iter().cloned().collect();
        let user = build_user_prompt(&history, seg.text.trim());
        let translated = self.backend.complete(&self.system_prompt, &user).await?;

        self.history.push_back(seg.text.trim().to_string());
        while self.history.len() > self.config.context_lines {
            self.history.pop_front();
        }

        Ok(TranslatedSegment {
            start_ms: seg.start_ms,
            end_ms: seg.end_ms,
            source_text: seg.text.trim().to_string(),
            translated_text: translated,
            source_lang: seg.lang.clone(),
            target_lang: self.config.target_lang.clone(),
        })
    }

    /// Run as a channel-driven task: read segments, emit translations.
    /// Segments that fail to translate are logged and skipped.
    pub async fn run(
        mut self,
        mut rx: mpsc::Receiver<TranscriptSegment>,
        tx: mpsc::Sender<TranslatedSegment>,
    ) {
        while let Some(seg) = rx.recv().await {
            if !self.should_translate(&seg) {
                continue;
            }
            match self.translate_segment(&seg).await {
                Ok(t) => {
                    if tx.send(t).await.is_err() {
                        break;
                    }
                }
                Err(e) => tracing::warn!("translate failed: {e}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::mock::MockBackend;
    use crate::error::TranslateError;

    fn seg(text: &str, start: u64, is_final: bool) -> TranscriptSegment {
        TranscriptSegment {
            start_ms: start,
            end_ms: start + 1000,
            text: text.into(),
            lang: Some("ja".into()),
            is_final,
        }
    }

    #[tokio::test]
    async fn translates_and_aligns_timestamps() {
        let backend = Arc::new(MockBackend::echo());
        let mut p = TranslatePipeline::new(
            backend,
            StreamerProfile::default(),
            TranslateConfig::default(),
        );
        let out = p.translate_segment(&seg("こんにちは", 5000, true)).await.unwrap();
        assert_eq!(out.start_ms, 5000);
        assert_eq!(out.end_ms, 6000);
        assert_eq!(out.source_text, "こんにちは");
        assert_eq!(out.translated_text, "T(こんにちは)");
        assert_eq!(out.target_lang, "zh");
    }

    #[tokio::test]
    async fn history_window_is_bounded_and_used() {
        let backend = Arc::new(MockBackend::echo());
        let mut cfg = TranslateConfig::default();
        cfg.context_lines = 2;
        let mut p =
            TranslatePipeline::new(backend.clone(), StreamerProfile::default(), cfg);

        for (i, line) in ["一", "二", "三", "四"].iter().enumerate() {
            p.translate_segment(&seg(line, i as u64 * 1000, true))
                .await
                .unwrap();
        }
        let calls = backend.calls.lock().unwrap();
        // 4th call should contain lines 二 and 三 as context but not 一.
        let last_user = &calls[3].1;
        assert!(last_user.contains("二"));
        assert!(last_user.contains("三"));
        assert!(!last_user.contains("一\n"));
    }

    #[tokio::test]
    async fn filters_partials_and_fillers() {
        let backend = Arc::new(MockBackend::echo());
        let p = TranslatePipeline::new(
            backend,
            StreamerProfile::default(),
            TranslateConfig::default(),
        );
        assert!(!p.should_translate(&seg("こんにちは", 0, false))); // partial
        assert!(!p.should_translate(&seg("あ", 0, true))); // too short
        assert!(p.should_translate(&seg("こんにちは", 0, true)));
    }

    #[tokio::test]
    async fn run_loop_skips_errors_and_continues() {
        let backend = Arc::new(MockBackend::scripted(vec![
            Err(TranslateError::EmptyResponse),
            Ok("好的".into()),
        ]));
        let p = TranslatePipeline::new(
            backend,
            StreamerProfile::default(),
            TranslateConfig::default(),
        );
        let (seg_tx, seg_rx) = mpsc::channel(8);
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let handle = tokio::spawn(p.run(seg_rx, out_tx));

        seg_tx.send(seg("失敗する行", 0, true)).await.unwrap();
        seg_tx.send(seg("成功する行", 1000, true)).await.unwrap();
        drop(seg_tx);

        let got = out_rx.recv().await.unwrap();
        assert_eq!(got.translated_text, "好的");
        assert_eq!(got.start_ms, 1000);
        assert!(out_rx.recv().await.is_none());
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn glossary_terms_reach_the_system_prompt() {
        use crate::glossary::{Glossary, GlossaryEntry};
        let backend = Arc::new(MockBackend::echo());
        let profile = StreamerProfile {
            name: "N".into(),
            description: String::new(),
            glossary: Glossary {
                entries: vec![GlossaryEntry {
                    source: "ぺこ".into(),
                    target: "呗贼".into(),
                    note: None,
                }],
            },
            references: vec![],
        };
        let mut p = TranslatePipeline::new(
            backend.clone(),
            profile,
            TranslateConfig::default(),
        );
        p.translate_segment(&seg("こんぺこ〜", 0, true)).await.unwrap();
        let calls = backend.calls.lock().unwrap();
        assert!(calls[0].0.contains("ぺこ => 呗贼"));
    }

    #[tokio::test]
    async fn set_profile_rebuilds_system_prompt() {
        let backend = Arc::new(MockBackend::echo());
        let mut p = TranslatePipeline::new(
            backend.clone(),
            StreamerProfile::default(),
            TranslateConfig::default(),
        );
        let mut new_profile = StreamerProfile::default();
        new_profile.name = "新主播".into();
        p.set_profile(new_profile);
        p.translate_segment(&seg("テスト", 0, true)).await.unwrap();
        let calls = backend.calls.lock().unwrap();
        assert!(calls[0].0.contains("新主播"));
    }
}
