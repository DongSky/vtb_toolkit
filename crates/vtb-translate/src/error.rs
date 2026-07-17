use thiserror::Error;

#[derive(Debug, Error)]
pub enum TranslateError {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("LLM API error ({status}): {body}")]
    Api { status: u16, body: String },

    #[error("empty response from backend")]
    EmptyResponse,

    #[error("invalid configuration: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, TranslateError>;
