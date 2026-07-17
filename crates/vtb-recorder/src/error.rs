use thiserror::Error;

#[derive(Debug, Error)]
pub enum RecorderError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("ffmpeg not found on PATH or at configured location")]
    FfmpegMissing,

    #[error("ffmpeg exited with status {0}")]
    FfmpegFailed(String),

    #[error("no playable stream url returned by the platform")]
    NoStreamUrl,

    #[error("recording already in progress")]
    AlreadyRecording,

    #[error("invalid configuration: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, RecorderError>;
