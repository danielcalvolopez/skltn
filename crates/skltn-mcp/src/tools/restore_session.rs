use std::path::Path;
use std::sync::{Arc, Mutex};

use tiktoken_rs::CoreBPE;

use crate::cache::{self, SkeletonCache};
use crate::resolve::resolve_safe_path;
use crate::session::SessionTracker;

use super::language_name;

/// Maximum total tokens to return in load mode.
const MAX_RESTORE_TOKENS: usize = 50_000;

#[derive(Debug, Clone, Copy)]
enum FileStatus {
    Unchanged,
    Modified,
    Removed,
}

impl std::fmt::Display for FileStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileStatus::Unchanged => write!(f, "unchanged"),
            FileStatus::Modified => write!(f, "modified since last session"),
            FileStatus::Removed => write!(f, "removed"),
        }
    }
}

struct ManifestEntry {
    file: String,
    status: FileStatus,
    language: String,
    token_estimate: usize,
}

/// Analyze a single file from the previous manifest to determine its status.
fn analyze_file(
    root: &Path,
    file: &str,
    skel_cache: &SkeletonCache,
    manifest_timestamp: &time::OffsetDateTime,
) -> ManifestEntry {
    let ext = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let language = language_name(ext).to_string();

    // Check if file still exists
    let path = match resolve_safe_path(root, file) {
        Ok(p) if p.is_file() => p,
        _ => {
            return ManifestEntry {
                file: file.to_string(),
                status: FileStatus::Removed,
                language,
                token_estimate: 0,
            };
        }
    };

    // Read current content and metadata
    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => {
            return ManifestEntry {
                file: file.to_string(),
                status: FileStatus::Removed,
                language,
                token_estimate: 0,
            };
        }
    };

    let mtime = std::fs::metadata(&path)
        .ok()
        .map(|m| cache::mtime_secs(&m))
        .unwrap_or(0);

    // Check cache to determine if content changed.
    // For files that were skeletonized, a cache entry exists and we can compare
    // content hashes. For files served full (below token threshold), no cache
    // entry is stored — fall back to mtime comparison against the manifest
    // timestamp.
    let token_estimate = source.len() / 4;
    let status = match skel_cache.get_with_validation(file, mtime, &source) {
        Some(entry) => {
            // Cache hit means content unchanged
            let _ = entry; // token info available but rough estimate is fine
            FileStatus::Unchanged
        }
        None => {
            // No cache hit — either content changed or the file was never cached
            // (served full). Use mtime: if the file was modified after the
            // manifest was written, treat it as modified.
            let manifest_epoch = manifest_timestamp
                .unix_timestamp();
            if mtime > manifest_epoch {
                FileStatus::Modified
            } else {
                FileStatus::Unchanged
            }
        }
    };

    ManifestEntry {
        file: file.to_string(),
        status,
        language,
        token_estimate,
    }
}

/// Build the table-of-contents response showing file statuses.
fn build_toc_response(entries: &[ManifestEntry], timestamp: &time::OffsetDateTime) -> String {
    let format = time::format_description::well_known::Rfc3339;
    let ts = timestamp.format(&format).unwrap_or_default();

    let mut out = String::new();
    out.push_str(&format!(
        "Previous session: {} files | {}\n\n",
        entries.len(),
        ts
    ));

    let mut total_tokens = 0usize;
    let mut unchanged = 0usize;
    let mut modified = 0usize;
    let mut removed = 0usize;

    for entry in entries {
        out.push_str(&format!(
            "  {} | {} | ~{} tokens | {}\n",
            entry.file, entry.language, entry.token_estimate, entry.status
        ));
        total_tokens += entry.token_estimate;
        match entry.status {
            FileStatus::Unchanged => unchanged += 1,
            FileStatus::Modified => modified += 1,
            FileStatus::Removed => removed += 1,
        }
    }

    out.push_str(&format!(
        "\nTotal: ~{} tokens | {} unchanged, {} modified, {} removed",
        total_tokens, unchanged, modified, removed
    ));

    out
}

/// Build the load response with actual file content.
fn build_load_response(
    root: &Path,
    tokenizer: &CoreBPE,
    entries: &[ManifestEntry],
    only_changed: bool,
    skel_cache: &SkeletonCache,
) -> String {
    let tracker = Arc::new(Mutex::new(SessionTracker::new()));
    let mut parts: Vec<String> = Vec::new();
    let mut total_tokens = 0usize;
    let mut omitted = 0usize;

    for entry in entries {
        // Skip removed files
        if matches!(entry.status, FileStatus::Removed) {
            continue;
        }

        // In only_changed mode, skip unchanged files
        if only_changed && matches!(entry.status, FileStatus::Unchanged) {
            continue;
        }

        // Check token budget
        if total_tokens + entry.token_estimate > MAX_RESTORE_TOKENS {
            omitted += 1;
            continue;
        }

        let content = super::read_skeleton::read_skeleton_or_full(
            root,
            &entry.file,
            tokenizer,
            &tracker,
            &None,
            Some(skel_cache),
            false, // record = false, don't pollute manifest
        );

        // Insert status tag into the header line
        let content = insert_status_tag(&content, entry.status);

        // Rough token count from the output
        total_tokens += entry.token_estimate;
        parts.push(content);
    }

    if parts.is_empty() {
        if only_changed {
            return "No changed files to restore from previous session.".to_string();
        }
        return "No files to restore from previous session.".to_string();
    }

    let mut result = parts.join("\n\n---\n\n");

    if omitted > 0 {
        result.push_str(&format!(
            "\n\n--- {} file(s) omitted due to token budget ({} limit) ---",
            omitted, MAX_RESTORE_TOKENS
        ));
    }

    result
}

/// Insert the file status tag into the header line before the closing `]`.
fn insert_status_tag(content: &str, status: FileStatus) -> String {
    if let Some(bracket_pos) = content.find(']') {
        let mut result = String::with_capacity(content.len() + 30);
        result.push_str(&content[..bracket_pos]);
        result.push_str(&format!(" | status: {}", status));
        result.push_str(&content[bracket_pos..]);
        result
    } else {
        content.to_string()
    }
}

/// Restore context from the previous session.
///
/// When `load` is false, returns a table-of-contents summary listing files,
/// their languages, estimated token counts, and change status (unchanged,
/// modified, removed).
///
/// When `load` is true, batch-loads file content using `read_skeleton_or_full`,
/// respecting a token budget of [`MAX_RESTORE_TOKENS`]. If `only_changed` is
/// also true, only files that have been modified since the last session are
/// included.
pub fn restore_session(
    root: &Path,
    tokenizer: &CoreBPE,
    skel_cache: &SkeletonCache,
    load: bool,
    only_changed: bool,
) -> String {
    let manifest = match skel_cache.load_previous_manifest() {
        Some(m) => m,
        None => return "No previous session found for this project.".to_string(),
    };

    if manifest.files.is_empty() {
        return "No previous session found for this project.".to_string();
    }

    let entries: Vec<ManifestEntry> = manifest
        .files
        .iter()
        .map(|file| analyze_file(root, file, skel_cache, &manifest.timestamp))
        .collect();

    if load {
        build_load_response(root, tokenizer, &entries, only_changed, skel_cache)
    } else {
        build_toc_response(&entries, &manifest.timestamp)
    }
}
