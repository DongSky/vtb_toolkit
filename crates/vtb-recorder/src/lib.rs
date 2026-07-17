//! Live stream recorder: open-status monitoring, ffmpeg-based pull
//! recording with automatic time/size segmentation, and post-stream export.

pub mod error;
pub mod ffmpeg;
pub mod stream;
pub mod monitor;
pub mod session;

pub use error::RecorderError;
pub use ffmpeg::{FfmpegRecorder, RecordOptions, SegmentPolicy};
pub use monitor::{Monitor, MonitorConfig, MonitorEvent, StatusSource};
pub use session::{RecordingSession, SessionMetadata};
