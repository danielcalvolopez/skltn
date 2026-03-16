use std::path::{Component, Path, PathBuf};

use crate::error::McpError;

/// Logically normalize a path by resolving `.` and `..` components without
/// touching the filesystem. This is used as a pre-check before canonicalization.
fn normalize_logical(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                if !components.is_empty() {
                    components.pop();
                }
            }
            Component::CurDir => {}
            other => components.push(other),
        }
    }
    components.iter().collect()
}

/// Resolves a relative path against a root directory, ensuring the result
/// stays within the root. Uses `canonicalize()` to resolve symlinks and `..`
/// segments, then checks that the canonical result starts with the canonical root.
///
/// If the candidate path does not exist, a logical normalization is performed
/// to detect path traversal even when `canonicalize()` would fail.
pub fn resolve_safe_path(root: &Path, relative: &str) -> Result<PathBuf, McpError> {
    let joined = root.join(relative);
    let canonical_root = root.canonicalize().map_err(|_| McpError::InvalidRoot)?;

    match joined.canonicalize() {
        Ok(canonical_candidate) => {
            if !canonical_candidate.starts_with(&canonical_root) {
                return Err(McpError::PathOutsideRoot);
            }
            Ok(canonical_candidate)
        }
        Err(_) => {
            // The path doesn't exist. Check whether logical normalization
            // reveals an escape from root before reporting FileNotFound.
            let normalized = normalize_logical(&joined);
            if !normalized.starts_with(&canonical_root) {
                // Also try comparing against the non-canonical root for cases
                // where the root itself has symlinks.
                let normalized_root = normalize_logical(root);
                if !normalized.starts_with(&normalized_root) {
                    return Err(McpError::PathOutsideRoot);
                }
            }
            Err(McpError::FileNotFound(relative.to_string()))
        }
    }
}
