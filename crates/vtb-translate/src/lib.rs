//! Streaming translation with per-streamer personalization.
//!
//! Pipeline: final ASR segments come in → context-aware prompt is built
//! (glossary pins terminology, reference translations steer style, rolling
//! history keeps coherence) → an LLM backend translates → aligned
//! [`vtb_common::TranslatedSegment`]s come out.

pub mod backend;
pub mod batch;
pub mod danmaku;
pub mod error;
pub mod glossary;
pub mod pipeline;
pub mod prompt;

pub use backend::{AnthropicBackend, LlmBackend, OpenAiCompatBackend};
pub use batch::{translate_batch, BatchConfig, UsageStats};
pub use error::TranslateError;
pub use glossary::{Glossary, GlossaryEntry, ReferencePair, StreamerProfile};
pub use pipeline::{TranslateConfig, TranslatePipeline};
pub mod settings;
