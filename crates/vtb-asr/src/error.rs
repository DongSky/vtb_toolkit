use thiserror::Error;

#[derive(Debug, Error)]
pub enum AsrError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("wav error: {0}")]
    Wav(#[from] hound::Error),

    #[error("ffmpeg failed: {0}")]
    Ffmpeg(String),

    #[error("model error: {0}")]
    Model(String),

    #[error("inference failed: {0}")]
    Inference(String),

    #[error("invalid configuration: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, AsrError>;
