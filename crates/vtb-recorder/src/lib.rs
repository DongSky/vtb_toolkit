//! Live stream recorder: open-status monitoring, ffmpeg-based pull
//! recording with automatic time/size segmentation, and post-stream export.

pub mod auto;
pub mod check;
pub mod disk;
pub mod error;
pub mod ffmpeg;
pub mod flv;
pub mod monitor;
pub mod platform;
pub mod retention;
pub mod session;
pub mod stream;
pub mod supervisor;

pub use auto::{AutoRecorder, AutoRecorderConfig, RecorderEvent, ResolvedStream, StreamResolver};
pub use check::{remux_mp4, verify_recording, RecordingHealth};
pub use error::RecorderError;
pub use ffmpeg::{FfmpegRecorder, RecordOptions, SegmentPolicy};
pub use monitor::{Monitor, MonitorConfig, MonitorEvent, StatusSource};
pub use session::{RecordingSession, SessionMetadata};

#[cfg(test)]
mod test_process;
