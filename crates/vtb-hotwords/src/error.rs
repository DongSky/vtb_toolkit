use thiserror::Error;

#[derive(Debug, Error)]
pub enum HotwordError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("table not found: {0}")]
    NotFound(String),

    #[error("invalid table name: {0}")]
    BadName(String),

    #[error("LLM normalization failed: {0}")]
    Normalize(String),
}

pub type Result<T> = std::result::Result<T, HotwordError>;
