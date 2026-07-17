//! Streaming translation with per-streamer personalization.
//!
//! Pipeline: final ASR segments come in → context-aware prompt is built
//! (glossary pins terminology, reference translations steer style, rolling
//! history keeps coherence) → an LLM backend translates → aligned
//! [`vtb_common::TranslatedSegment`]s come out.

pub mod error;
pub mod glossary;
pub mod prompt;
pub mod backend;
pub mod pipeline;

pub use backend::{LlmBackend, OpenAiCompatBackend, AnthropicBackend};
pub use error::TranslateError;
pub use glossary::{Glossary, GlossaryEntry, ReferencePair, StreamerProfile};
pub use pipeline::{TranslatePipeline, TranslateConfig};
