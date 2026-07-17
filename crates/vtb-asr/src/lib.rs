//! Speech-to-text for live streams and recordings.
//!
//! Architecture:
//! - [`audio`]: PCM utilities — WAV IO, resampling to 16 kHz mono f32,
//!   ffmpeg-based audio extraction from any container.
//! - [`vad`]: energy-based voice activity detection with hangover, used to
//!   chunk continuous audio into utterances.
//! - [`engine`]: the [`AsrEngine`] trait; local whisper.cpp backend
//!   (feature `whisper`), mock for tests.
//! - [`streaming`]: feeds live PCM through VAD and an engine, emitting
//!   [`vtb_common::TranscriptSegment`]s over a channel.

pub mod error;
pub mod audio;
pub mod vad;
pub mod engine;
pub mod streaming;
pub mod models;

pub use engine::AsrEngine;
pub use error::AsrError;
pub use streaming::{StreamingAsr, StreamingConfig};

/// Target sample rate for all ASR processing.
pub const SAMPLE_RATE: u32 = 16_000;
