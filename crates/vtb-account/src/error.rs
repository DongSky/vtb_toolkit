use thiserror::Error;

#[derive(Debug, Error)]
pub enum AccountError {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("bilibili API code {code}: {message}")]
    Api { code: i64, message: String },

    #[error("QR code expired")]
    QrExpired,

    #[error("login response missing field: {0}")]
    MissingField(&'static str),

    #[error("keychain error: {0}")]
    Keyring(String),

    #[error("not logged in")]
    NotLoggedIn,
}

pub type Result<T> = std::result::Result<T, AccountError>;
