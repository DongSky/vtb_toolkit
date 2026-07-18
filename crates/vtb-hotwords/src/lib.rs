//! Hotword tables (热词表) for ASR accuracy and translation consistency.
//!
//! Recorders describe their terminology in ANY free-form text — player
//! names, hero aliases, memes, "GPK是打野选手, 帕克=Puck" — and it gets
//! normalized into a unified table format:
//!
//! - [`parse`]: dependency-free rule parser for common formats
//! - [`normalize`]: LLM-powered normalizer for messy natural language
//!   (falls back to the rule parser)
//! - [`store`]: one JSON file per table; save/load/list/delete/merge
//! - [`prompt`]: builders feeding whisper's initial prompt and the
//!   translation glossary

pub mod error;
pub mod types;
pub mod parse;
pub mod normalize;
pub mod store;
pub mod prompt;

pub use error::HotwordError;
pub use normalize::{normalize_with_llm, TextCompleter};
pub use parse::parse_rules;
pub use prompt::{asr_initial_prompt, translation_pairs};
pub use store::TableStore;
pub use types::{HotwordEntry, HotwordTable};
