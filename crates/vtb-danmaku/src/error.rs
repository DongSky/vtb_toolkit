use thiserror::Error;

#[derive(Debug, Error)]
pub enum DanmakuError {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("websocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("packet too short: need {need} bytes, got {got}")]
    ShortPacket { need: usize, got: usize },

    #[error("unsupported protocol version: {0}")]
    UnsupportedProtocol(u16),

    #[error("decompression failed: {0}")]
    Decompress(String),

    #[error("bilibili API returned error code {code}: {message}")]
    Api { code: i64, message: String },

    #[error("missing field in API response: {0}")]
    MissingField(&'static str),

    #[error("no danmaku host available")]
    NoHost,
}

pub type Result<T> = std::result::Result<T, DanmakuError>;
