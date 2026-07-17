use thiserror::Error;

#[derive(Debug, Error)]
pub enum NotifyError {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("delivery failed: {0}")]
    Delivery(String),
}

pub type Result<T> = std::result::Result<T, NotifyError>;
