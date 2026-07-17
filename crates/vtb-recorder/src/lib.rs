//! Live stream recorder: open-status monitoring, ffmpeg-based pull
//! recording with automatic time/size segmentation, and post-stream export.

pub mod error;
pub mod ffmpeg;
pub mod stream;
pub mod monitor;
pub mod session;
pub mod auto;

pub use auto::{AutoRecorder, AutoRecorderConfig, RecorderEvent, ResolvedStream, StreamResolver};
pub use error::RecorderError;
pub use ffmpeg::{FfmpegRecorder, RecordOptions, SegmentPolicy};
pub use monitor::{Monitor, MonitorConfig, MonitorEvent, StatusSource};
pub use session::{RecordingSession, SessionMetadata};
