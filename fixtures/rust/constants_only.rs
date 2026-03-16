//! Configuration constants for the skeleton engine.

pub const MAX_FILE_SIZE: usize = 1_048_576;
pub const DEFAULT_MAX_DEPTH: usize = 100;
pub const SUPPORTED_EXTENSIONS: &[&str] = &["rs", "py", "ts", "js"];
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The default tag prefix used in skeleton output.
pub const TAG_PREFIX: &str = "skltn";
