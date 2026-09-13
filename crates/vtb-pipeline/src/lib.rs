//! Offline processing pipeline: recording file → subtitles, translation,
//! highlights, clips.

pub mod ai_clips;
pub mod danmaku_log;
pub mod danmaku_xml;
pub mod energy;
pub mod error;
pub mod job;
pub mod markers;
pub mod subtitle;

pub use ai_clips::AiClipConfig;
pub use error::PipelineError;
pub use job::{JobConfig, JobProgress, OfflineJob, Stage};
