use std::fmt;

#[derive(Debug)]
pub enum McpError {
    InvalidRoot,
    FileNotFound(String),
    PathOutsideRoot,
    UnsupportedLanguage(String),
    SymbolNotFound { name: String, file: String },
    DirectoryNotFound(String),
    PathIsFile(String),
    NoSupportedFiles(String),
    Core(skltn_core::error::SkltnError),
}

impl fmt::Display for McpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            McpError::InvalidRoot => write!(f, "Invalid repository root path"),
            McpError::FileNotFound(path) => write!(f, "File not found: {path}"),
            McpError::PathOutsideRoot => write!(f, "Path is outside the repository root"),
            McpError::UnsupportedLanguage(path) => {
                write!(
                    f,
                    "Unsupported language for file: {path}. Supported: .rs, .py, .ts, .js"
                )
            }
            McpError::SymbolNotFound { name, file } => {
                write!(f, "Symbol '{name}' not found in {file}")
            }
            McpError::DirectoryNotFound(path) => write!(f, "Directory not found: {path}"),
            McpError::PathIsFile(path) => {
                write!(
                    f,
                    "Path is a file, not a directory: {path}. Use read_skeleton to inspect it."
                )
            }
            McpError::NoSupportedFiles(path) => {
                write!(
                    f,
                    "No supported source files (.rs, .py, .ts, .js) found in {path}"
                )
            }
            McpError::Core(e) => write!(f, "Engine error: {e}"),
        }
    }
}

impl From<skltn_core::error::SkltnError> for McpError {
    fn from(e: skltn_core::error::SkltnError) -> Self {
        McpError::Core(e)
    }
}
