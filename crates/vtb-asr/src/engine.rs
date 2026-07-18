//! ASR engines: the trait, a whisper.cpp backend, and a test mock.

use crate::error::Result;
use async_trait::async_trait;

/// A recognized utterance (engine-level, before channel plumbing).
#[derive(Debug, Clone, PartialEq)]
pub struct Recognition {
    pub text: String,
    /// Detected language tag ("zh"/"en"/"ja"/...), if the engine knows.
    pub lang: Option<String>,
}

#[async_trait]
pub trait AsrEngine: Send + Sync {
    /// Transcribe one utterance of 16 kHz mono f32 PCM.
    async fn transcribe(&self, pcm: &[f32]) -> Result<Recognition>;
}

#[cfg(feature = "whisper")]
pub use whisper_impl::WhisperEngine;

#[cfg(feature = "whisper")]
mod whisper_impl {
    use super::*;
    use crate::error::AsrError;
    use std::path::Path;
    use std::sync::Mutex;
    use whisper_rs::{
        FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters,
        WhisperState,
    };

    /// Local whisper.cpp engine. Multilingual ggml models handle zh/en/ja.
    pub struct WhisperEngine {
        state: Mutex<WhisperState>,
        /// Language hint; `None` = auto-detect per utterance.
        language: Option<String>,
        /// Vocabulary/context prompt (热词表) nudging recognition toward
        /// specific spellings — proper nouns, player IDs, memes.
        initial_prompt: Option<String>,
        n_threads: i32,
    }

    impl WhisperEngine {
        /// Set the vocabulary prompt (builder style).
        pub fn with_initial_prompt(mut self, prompt: impl Into<String>) -> Self {
            let p = prompt.into();
            self.initial_prompt = (!p.trim().is_empty()).then_some(p);
            self
        }

        pub fn new(model_path: &Path, language: Option<String>) -> Result<Self> {
            let ctx = WhisperContext::new_with_params(
                model_path
                    .to_str()
                    .ok_or_else(|| AsrError::Config("model path not utf-8".into()))?,
                WhisperContextParameters::default(),
            )
            .map_err(|e| AsrError::Model(format!("load: {e}")))?;
            let state = ctx
                .create_state()
                .map_err(|e| AsrError::Model(format!("state: {e}")))?;
            let n_threads = std::thread::available_parallelism()
                .map(|n| (n.get() as i32).min(8))
                .unwrap_or(4);
            Ok(Self {
                state: Mutex::new(state),
                language,
                initial_prompt: None,
                n_threads,
            })
        }

        fn run(&self, pcm: &[f32]) -> Result<Recognition> {
            // whisper.cpp degrades badly on utterances shorter than ~1 s;
            // pad with trailing silence to 1.1 s minimum.
            const MIN_SAMPLES: usize = (crate::SAMPLE_RATE as usize * 11) / 10;
            let padded;
            let pcm = if pcm.len() < MIN_SAMPLES {
                padded = {
                    let mut v = pcm.to_vec();
                    v.resize(MIN_SAMPLES, 0.0);
                    v
                };
                &padded[..]
            } else {
                pcm
            };

            let mut state = self.state.lock().unwrap();
            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            params.set_n_threads(self.n_threads);
            params.set_translate(false);
            match &self.language {
                Some(l) => params.set_language(Some(l)),
                None => params.set_language(Some("auto")),
            }
            params.set_print_special(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);
            params.set_suppress_blank(true);
            params.set_no_context(true);
            if let Some(prompt) = &self.initial_prompt {
                params.set_initial_prompt(prompt);
            }

            state
                .full(params, pcm)
                .map_err(|e| AsrError::Inference(format!("{e}")))?;

            let n = state
                .full_n_segments()
                .map_err(|e| AsrError::Inference(format!("{e}")))?;
            let mut text = String::new();
            for i in 0..n {
                if let Ok(seg) = state.full_get_segment_text(i) {
                    text.push_str(seg.trim());
                    if !text.is_empty() {
                        text.push(' ');
                    }
                }
            }
            let lang_id =
                whisper_rs::get_lang_str(state.full_lang_id_from_state().unwrap_or(-1))
                    .map(str::to_string);
            Ok(Recognition {
                text: text.trim().to_string(),
                lang: lang_id,
            })
        }
    }

    #[async_trait]
    impl AsrEngine for WhisperEngine {
        async fn transcribe(&self, pcm: &[f32]) -> Result<Recognition> {
            // whisper.cpp is CPU/GPU-bound sync code; yield the runtime
            // thread to the blocking pool while it runs. Requires a
            // multi-threaded runtime (the app and tests use one).
            tokio::task::block_in_place(|| self.run(pcm))
        }
    }
}

pub mod mock {
    use super::*;
    use std::sync::Mutex;

    /// Deterministic mock: returns scripted texts in order; falls back to
    /// a length-derived placeholder.
    pub struct MockEngine {
        pub scripted: Mutex<Vec<Recognition>>,
        pub calls: Mutex<Vec<usize>>, // pcm lengths
    }

    impl MockEngine {
        pub fn new(scripted: Vec<Recognition>) -> Self {
            Self {
                scripted: Mutex::new(scripted),
                calls: Mutex::new(vec![]),
            }
        }

        pub fn empty() -> Self {
            Self::new(vec![])
        }
    }

    #[async_trait]
    impl AsrEngine for MockEngine {
        async fn transcribe(&self, pcm: &[f32]) -> Result<Recognition> {
            self.calls.lock().unwrap().push(pcm.len());
            let mut s = self.scripted.lock().unwrap();
            if s.is_empty() {
                Ok(Recognition {
                    text: format!("utt-{}", pcm.len()),
                    lang: Some("zh".into()),
                })
            } else {
                Ok(s.remove(0))
            }
        }
    }
}
