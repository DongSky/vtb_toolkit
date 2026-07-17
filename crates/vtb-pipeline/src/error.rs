use thiserror::Error;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("asr error: {0}")]
    Asr(#[from] vtb_asr::AsrError),

    #[error("translate error: {0}")]
    Translate(#[from] vtb_translate::TranslateError),

    #[error("highlight error: {0}")]
    Highlight(#[from] vtb_highlight::HighlightError),

    #[error("invalid configuration: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, PipelineError>;
