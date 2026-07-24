//! Offline processing pipeline: recording file → subtitles, translation,
//! highlights, clips.

pub mod error;
pub mod subtitle;
pub mod energy;
pub mod danmaku_log;
pub mod danmaku_xml;
pub mod markers;
pub mod job;

pub use error::PipelineError;
pub use job::{OfflineJob, JobConfig, JobProgress, Stage};
