//! Batch translation: bounded concurrency with order-preserving output,
//! per-segment retry with exponential backoff, and usage accounting.
//!
//! Unlike the streaming [`crate::TranslatePipeline`] (which feeds each
//! translation's source line into a rolling history), concurrent requests
//! cannot share serially-updated context. Instead the context is *pre-filled*
//! deterministically: each segment's context is the up-to
//! `TranslateConfig::context_lines` transcript lines immediately preceding it
//! in the original transcript, formatted exactly like the streaming prompt.

use crate::backend::LlmBackend;
use crate::pipeline::{should_translate, TranslateConfig};
use crate::glossary::StreamerProfile;
use crate::prompt::{build_system_prompt, build_user_prompt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;
use vtb_common::{TranscriptSegment, TranslatedSegment};

#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum number of in-flight LLM requests.
    pub concurrency: usize,
    /// Retries per segment after the first attempt fails.
    pub max_retries: u32,
    /// Delay before the first retry; doubles after each further failure.
    pub retry_delay: Duration,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            concurrency: 4,
            max_retries: 2,
            retry_delay: Duration::from_millis(500),
        }
    }
}

/// Aggregate LLM usage of one batch run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageStats {
    /// LLM requests issued, including retries.
    pub requests: u64,
    /// Requests beyond the first per segment.
    pub retries: u64,
    /// Segments dropped after exhausting all retries.
    pub failures: u64,
    /// Source characters submitted (counted once per attempted segment).
    pub chars_in: u64,
    /// Translated characters received.
    pub chars_out: u64,
}

struct TaskOutcome {
    slot: usize,
    translated: Option<TranslatedSegment>,
    requests: u64,
    chars_in: u64,
    chars_out: u64,
}

/// Translate every segment of `segments` that passes
/// [`should_translate`], with at most `batch_cfg.concurrency` concurrent
/// requests. Output preserves the original transcript order. Segments whose
/// retries are exhausted are skipped (counted in [`UsageStats::failures`]).
/// `progress` receives `(done, total)` after each segment completes, where
/// `total` is the number of translatable segments.
pub async fn translate_batch(
    backend: Arc<dyn LlmBackend>,
    profile: &StreamerProfile,
    cfg: &TranslateConfig,
    batch_cfg: &BatchConfig,
    segments: &[TranscriptSegment],
    progress: Option<mpsc::Sender<(usize, usize)>>,
) -> (Vec<TranslatedSegment>, UsageStats) {
    let system_prompt = Arc::new(build_system_prompt(profile, &cfg.target_lang));

    // Pre-fill deterministic context per translatable segment.
    let mut jobs: Vec<(usize, TranscriptSegment, Vec<String>)> = Vec::new();
    for (i, seg) in segments.iter().enumerate() {
        if !should_translate(cfg, seg) {
            continue;
        }
        let lo = i.saturating_sub(cfg.context_lines);
        let context: Vec<String> = segments[lo..i]
            .iter()
            .map(|s| s.text.trim().to_string())
            .collect();
        jobs.push((jobs.len(), seg.clone(), context));
    }

    let total = jobs.len();
    let mut slots: Vec<Option<TranslatedSegment>> = vec![None; total];
    let mut stats = UsageStats::default();

    let sem = Arc::new(Semaphore::new(batch_cfg.concurrency.max(1)));
    let mut set = JoinSet::new();
    for (slot, seg, context) in jobs {
        let backend = backend.clone();
        let system = system_prompt.clone();
        let sem = sem.clone();
        let target_lang = cfg.target_lang.clone();
        let max_retries = batch_cfg.max_retries;
        let base_delay = batch_cfg.retry_delay;
        set.spawn(async move {
            let _permit = sem.acquire_owned().await.expect("semaphore never closed");
            let text = seg.text.trim().to_string();
            let user = build_user_prompt(&context, &text);
            let mut requests = 0u64;
            let mut delay = base_delay;
            let mut translated = None;
            for attempt in 0..=max_retries {
                requests += 1;
                match backend.complete(&system, &user).await {
                    Ok(t) => {
                        translated = Some(t);
                        break;
                    }
                    Err(e) if attempt < max_retries => {
                        tracing::warn!(
                            "translate segment @{}ms failed (attempt {}): {e}; retrying",
                            seg.start_ms,
                            attempt + 1
                        );
                        tokio::time::sleep(delay).await;
                        delay *= 2;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "translate segment @{}ms failed after {requests} attempts: {e}; skipping",
                            seg.start_ms
                        );
                    }
                }
            }
            let chars_in = text.chars().count() as u64;
            let (translated, chars_out) = match translated {
                Some(t) => {
                    let chars_out = t.chars().count() as u64;
                    let out = TranslatedSegment {
                        start_ms: seg.start_ms,
                        end_ms: seg.end_ms,
                        source_text: text,
                        translated_text: t,
                        source_lang: seg.lang.clone(),
                        target_lang,
                    };
                    (Some(out), chars_out)
                }
                None => (None, 0),
            };
            TaskOutcome { slot, translated, requests, chars_in, chars_out }
        });
    }

    let mut done = 0usize;
    while let Some(res) = set.join_next().await {
        done += 1;
        match res {
            Ok(o) => {
                stats.requests += o.requests;
                stats.retries += o.requests.saturating_sub(1);
                stats.chars_in += o.chars_in;
                stats.chars_out += o.chars_out;
                match o.translated {
                    Some(t) => slots[o.slot] = Some(t),
                    None => stats.failures += 1,
                }
            }
            Err(e) => {
                tracing::warn!("translate task panicked: {e}");
                stats.failures += 1;
            }
        }
        if let Some(tx) = &progress {
            let _ = tx.send((done, total)).await;
        }
    }

    (slots.into_iter().flatten().collect(), stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::mock::MockBackend;
    use crate::error::{Result, TranslateError};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn seg(text: &str, i: u64) -> TranscriptSegment {
        TranscriptSegment {
            start_ms: i * 1000,
            end_ms: i * 1000 + 1000,
            text: text.into(),
            lang: Some("ja".into()),
            is_final: true,
        }
    }

    #[tokio::test]
    async fn preserves_order_and_prefilled_context() {
        let backend = Arc::new(MockBackend::echo());
        let segs: Vec<_> = (0..10).map(|i| seg(&format!("行{i}"), i)).collect();
        let cfg = TranslateConfig {
            context_lines: 3,
            ..Default::default()
        };
        let bcfg = BatchConfig::default(); // concurrency 4
        let (out, stats) = translate_batch(
            backend.clone(),
            &StreamerProfile::default(),
            &cfg,
            &bcfg,
            &segs,
            None,
        )
        .await;

        // Order and alignment preserved despite concurrency.
        assert_eq!(out.len(), 10);
        for (i, t) in out.iter().enumerate() {
            assert_eq!(t.source_text, format!("行{i}"));
            assert_eq!(t.translated_text, format!("T(行{i})"));
            assert_eq!(t.start_ms, i as u64 * 1000);
            assert_eq!(t.target_lang, "zh");
        }

        // Pre-filled context: request for 行5 carries 行2..行4, not 行1.
        let calls = backend.calls.lock().unwrap();
        let mid = calls.iter().find(|(_, u)| u.ends_with("行5")).unwrap();
        assert!(mid.1.contains("行2\n行3\n行4"));
        assert!(!mid.1.contains("行1"));
        // First segment has no context at all.
        let first = calls.iter().find(|(_, u)| u.ends_with("行0")).unwrap();
        assert!(first.1.starts_with("Translate this line:"));

        assert_eq!(stats.requests, 10);
        assert_eq!(stats.retries, 0);
        assert_eq!(stats.failures, 0);
        assert_eq!(stats.chars_in, 20); // 10 × "行N" (2 chars)
        assert_eq!(stats.chars_out, 50); // 10 × "T(行N)" (5 chars)
    }

    #[tokio::test(start_paused = true)]
    async fn retries_then_succeeds() {
        let backend = Arc::new(MockBackend::scripted(vec![
            Err(TranslateError::EmptyResponse),
            Err(TranslateError::EmptyResponse),
            Ok("成功".into()),
        ]));
        let segs = vec![seg("よろしくお願いします", 0)];
        let (out, stats) = translate_batch(
            backend,
            &StreamerProfile::default(),
            &TranslateConfig::default(),
            &BatchConfig::default(), // max_retries 2
            &segs,
            None,
        )
        .await;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].translated_text, "成功");
        assert_eq!(stats.requests, 3);
        assert_eq!(stats.retries, 2);
        assert_eq!(stats.failures, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn exhausted_retries_skip_segment_others_survive() {
        // concurrency 1 → the first segment deterministically consumes the
        // three scripted errors; the rest fall through to echo mode.
        let backend = Arc::new(MockBackend::scripted(vec![
            Err(TranslateError::EmptyResponse),
            Err(TranslateError::EmptyResponse),
            Err(TranslateError::EmptyResponse),
        ]));
        let segs = vec![seg("だめな行", 0), seg("いい行", 1), seg("もう一行", 2)];
        let bcfg = BatchConfig {
            concurrency: 1,
            ..Default::default()
        };
        let (out, stats) = translate_batch(
            backend,
            &StreamerProfile::default(),
            &TranslateConfig::default(),
            &bcfg,
            &segs,
            None,
        )
        .await;
        assert_eq!(out.len(), 2, "failed segment is skipped");
        assert_eq!(out[0].source_text, "いい行");
        assert_eq!(out[1].source_text, "もう一行");
        assert_eq!(stats.failures, 1);
        assert_eq!(stats.retries, 2);
        assert_eq!(stats.requests, 5); // 3 for the failure + 1 each after
    }

    #[tokio::test]
    async fn progress_and_stats_with_skipped_segments() {
        let backend = Arc::new(MockBackend::echo());
        let mut zh = seg("你好你好", 3);
        zh.lang = Some("zh".into()); // matches target → skipped
        let mut partial = seg("途中経過", 4);
        partial.is_final = false; // skipped
        let segs = vec![
            seg("こんにちは", 0),
            seg("あ", 1), // too short → skipped
            seg("やっていくぞ", 2),
            zh,
            partial,
            seg("最後の行", 5),
        ];
        let (tx, mut rx) = mpsc::channel(16);
        let (out, stats) = translate_batch(
            backend,
            &StreamerProfile::default(),
            &TranslateConfig::default(),
            &BatchConfig::default(),
            &segs,
            Some(tx),
        )
        .await;

        assert_eq!(out.len(), 3);
        assert_eq!(out[0].source_text, "こんにちは");
        assert_eq!(out[1].source_text, "やっていくぞ");
        assert_eq!(out[2].source_text, "最後の行");

        let expected_in: u64 = ["こんにちは", "やっていくぞ", "最後の行"]
            .iter()
            .map(|s| s.chars().count() as u64)
            .sum();
        assert_eq!(stats.requests, 3);
        assert_eq!(stats.retries, 0);
        assert_eq!(stats.failures, 0);
        assert_eq!(stats.chars_in, expected_in);
        assert_eq!(stats.chars_out, expected_in + 3 * 3); // "T(" + ")" per line

        // Progress: (1,3) (2,3) (3,3) in order, channel closed after.
        let mut got = Vec::new();
        while let Some(p) = rx.recv().await {
            got.push(p);
        }
        assert_eq!(got, vec![(1, 3), (2, 3), (3, 3)]);
    }

    /// Backend that tracks the peak number of concurrent `complete` calls.
    struct GaugeBackend {
        current: AtomicUsize,
        peak: AtomicUsize,
    }

    #[async_trait]
    impl LlmBackend for GaugeBackend {
        async fn complete(&self, _system: &str, user: &str) -> Result<String> {
            let now = self.current.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
            self.current.fetch_sub(1, Ordering::SeqCst);
            Ok(format!("T({})", user.lines().last().unwrap_or("")))
        }
    }

    #[tokio::test(start_paused = true)]
    async fn concurrency_never_exceeds_limit() {
        let backend = Arc::new(GaugeBackend {
            current: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        });
        let segs: Vec<_> = (0..10).map(|i| seg(&format!("行{i}"), i)).collect();
        let bcfg = BatchConfig {
            concurrency: 3,
            ..Default::default()
        };
        let (out, stats) = translate_batch(
            backend.clone(),
            &StreamerProfile::default(),
            &TranslateConfig::default(),
            &bcfg,
            &segs,
            None,
        )
        .await;
        assert_eq!(out.len(), 10);
        assert_eq!(stats.requests, 10);
        let peak = backend.peak.load(Ordering::SeqCst);
        assert!(peak <= 3, "peak concurrency {peak} exceeded limit 3");
        assert!(peak >= 2, "expected actual concurrency, got peak {peak}");
    }

    #[tokio::test]
    async fn empty_and_untranslatable_input_yields_defaults() {
        let backend = Arc::new(MockBackend::echo());
        let (out, stats) = translate_batch(
            backend.clone(),
            &StreamerProfile::default(),
            &TranslateConfig::default(),
            &BatchConfig::default(),
            &[],
            None,
        )
        .await;
        assert!(out.is_empty());
        assert_eq!(stats, UsageStats::default());
        assert!(backend.calls.lock().unwrap().is_empty());
    }
}
