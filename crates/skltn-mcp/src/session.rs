use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::budget::CacheHint;

/// Tracks which files have been served full in the current MCP session.
/// Used to produce `CacheHint::RecentlyServed` for files likely in the
/// provider's prompt cache.
///
/// The tracker's lifetime matches the MCP server process — no eviction needed.
#[derive(Default)]
pub struct SessionTracker {
    served_full: HashMap<PathBuf, Instant>,
}

impl SessionTracker {
    pub fn new() -> Self {
        Self {
            served_full: HashMap::new(),
        }
    }

    /// Record that a file was served full (not skeletonized).
    pub fn record_full(&mut self, path: &Path) {
        self.served_full.insert(path.to_path_buf(), Instant::now());
    }

    /// Get a cache hint for a file based on session history.
    pub fn hint_for(&self, path: &Path) -> CacheHint {
        if self.served_full.contains_key(path) {
            CacheHint::RecentlyServed
        } else {
            CacheHint::Unknown
        }
    }
}
