use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkltnError {
    #[error("unsupported language for extension: {0}")]
    UnsupportedLanguage(String),

    #[error("failed to parse source: {0}")]
    ParseError(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
