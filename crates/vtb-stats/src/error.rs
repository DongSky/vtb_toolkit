use thiserror::Error;

#[derive(Debug, Error)]
pub enum StatsError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid timestamp in database: {0}")]
    Timestamp(String),
}

pub type Result<T> = std::result::Result<T, StatsError>;
