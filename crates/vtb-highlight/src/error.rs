use thiserror::Error;

#[derive(Debug, Error)]
pub enum HighlightError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("ffmpeg failed: {0}")]
    Ffmpeg(String),

    #[error("multimodal API error ({status}): {body}")]
    Api { status: u16, body: String },

    #[error("invalid configuration: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, HighlightError>;
