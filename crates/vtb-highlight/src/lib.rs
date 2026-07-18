//! Highlight detection: multi-signal fusion over a live stream or VOD.
//!
//! Signals:
//! 1. danmaku density peaks (z-score over sliding windows)
//! 2. emotion keywords in danmaku (草 / w / 哈哈哈 / ？？？ ...)
//! 3. gift & SuperChat bursts
//! 4. audio energy (RMS series supplied by the pipeline)
//! 5. optional multimodal LLM rescoring of candidate windows
//!
//! Fusion produces scored windows; adjacent hot windows merge into
//! [`vtb_common::Highlight`] items ready for one-click clipping.

pub mod error;
pub mod signals;
pub mod fusion;
pub mod clip;
pub mod multimodal;
pub mod realtime;
pub mod music;
pub mod export;

pub use error::HighlightError;
pub use fusion::{FusionConfig, detect_highlights};
pub use signals::{WindowSeries, danmaku_density, keyword_score, gift_value, audio_energy_scores};
