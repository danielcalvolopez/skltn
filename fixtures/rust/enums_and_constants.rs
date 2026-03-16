pub const MAX_FILE_SIZE: usize = 1_048_576;
pub const DEFAULT_DEPTH: usize = 100;

/// Errors that can occur during parsing.
#[derive(Debug)]
pub enum ParseError {
    UnsupportedLanguage(String),
    SyntaxError { line: usize, col: usize },
    IoError(std::io::Error),
}

/// Supported programming languages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Language {
    Rust,
    Python,
    TypeScript,
    JavaScript,
}

impl Language {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Self::Rust),
            "py" => Some(Self::Python),
            "ts" => Some(Self::TypeScript),
            "js" => Some(Self::JavaScript),
            _ => None,
        }
    }
}
