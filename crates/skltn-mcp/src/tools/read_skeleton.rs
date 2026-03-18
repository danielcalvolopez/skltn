use std::path::Path;
use std::sync::{Arc, Mutex};

use tiktoken_rs::CoreBPE;
use time::OffsetDateTime;

use skltn_core::engine::SkeletonEngine;
use skltn_core::options::SkeletonOptions;

use crate::budget::{self, BudgetDecision};
use crate::cache::{self, SkeletonCache};
use crate::error::McpError;
use crate::resolve::resolve_safe_path;
use crate::savings::{SavingsRecord, SavingsWriter};
use crate::session::SessionTracker;

use super::{backend_for_extension, has_parse_errors, language_name};

/// Maximum file size we will attempt to read (10 MB).
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Read a source file, returning either its full content (if small or
/// previously served full) or a skeletonized summary (if large and first read).
///
/// The `tracker` records files served full so subsequent reads can skip
/// skeletonization (prompt cache economics — see spec amendment 2026-03-17).
///
/// The returned string includes a metadata header line followed by the content.
/// Error cases return a human-readable error message string.
pub fn read_skeleton_or_full(
    root: &Path,
    file: &str,
    tokenizer: &CoreBPE,
    tracker: &Arc<Mutex<SessionTracker>>,
    savings_writer: &Option<SavingsWriter>,
    skeleton_cache: Option<&SkeletonCache>,
    record: bool,
) -> String {
    // Resolve and validate path
    let path = match resolve_safe_path(root, file) {
        Ok(p) => p,
        Err(e) => return e.to_string(),
    };

    // Check it is a file
    if !path.is_file() {
        return McpError::FileNotFound(file.to_string()).to_string();
    }

    // Guard against very large files
    if let Ok(metadata) = std::fs::metadata(&path) {
        if metadata.len() > MAX_FILE_SIZE {
            return format!(
                "File too large: {} ({} bytes, limit is 10 MB)",
                file,
                metadata.len()
            );
        }
    }

    // Detect language via extension
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let backend = match backend_for_extension(ext) {
        Some(b) => b,
        None => return McpError::UnsupportedLanguage(file.to_string()).to_string(),
    };

    // Read file contents
    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return McpError::FileNotFound(file.to_string()).to_string(),
    };

    let lang = language_name(ext);

    // Get cache hint from session tracker
    let hint = tracker.lock().unwrap().hint_for(&path);

    match budget::should_skeletonize(&source, tokenizer, hint) {
        BudgetDecision::ReturnFull { original_tokens } => {
            // Tag as cache-aware only when the hint actually changed the
            // decision — i.e., the file is above the token threshold and would
            // have been skeletonized without the hint. Small files under the
            // threshold are always served full regardless of hint.
            let cache_aware = original_tokens > budget::TOKEN_THRESHOLD
                && matches!(
                    hint,
                    crate::budget::CacheHint::RecentlyServed
                        | crate::budget::CacheHint::CacheConfirmed
                );

            // Record this file as served full for future cache hints.
            // We record even small files — they are genuinely in the provider's
            // cache after being served, and if the file grows between reads
            // (e.g., user adds code), the hint will correctly prevent
            // skeletonization of the now-larger file.
            tracker.lock().unwrap().record_full(&path);

            // Record savings (saved_tokens = 0) so the dashboard counts this file
            if let Some(writer) = savings_writer.as_ref() {
                writer.record(SavingsRecord {
                    timestamp: OffsetDateTime::now_utc(),
                    file: file.to_string(),
                    language: lang.to_string(),
                    original_tokens,
                    skeleton_tokens: original_tokens,
                    saved_tokens: 0,
                });
            }

            if record {
                if let Some(cache) = skeleton_cache {
                    cache.record_manifest_entry(file);
                }
            }

            let cache_tag = if cache_aware { " (cache-aware)" } else { "" };
            let warning = if has_parse_errors(&source, backend.as_ref()) {
                " | warning: parse errors detected"
            } else {
                ""
            };

            format!(
                "[file: {file} | language: {lang} | tokens: {original_tokens} | full file{cache_tag}{warning}]\n\n{source}"
            )
        }
        BudgetDecision::Skeletonize { original_tokens } => {
            // Skeletonized files are NOT recorded in the session tracker.
            // The skeleton token sequence differs from the full file, so it
            // wouldn't benefit from the provider's prompt cache of the full file.

            // Check the persistent skeleton cache before doing AST work
            let mtime = std::fs::metadata(&path)
                .ok()
                .map(|m| cache::mtime_secs(&m))
                .unwrap_or(0);

            let (skeleton, skeleton_tokens, parse_errors) =
                if let Some(entry) = skeleton_cache.and_then(|c| c.get_with_validation(file, mtime, &source)) {
                    (entry.skeleton, entry.skeleton_tokens, entry.has_parse_errors)
                } else {
                    // Cache miss — skeletonize from scratch
                    let opts = SkeletonOptions::default();
                    let skel = match SkeletonEngine::skeletonize(&source, backend.as_ref(), &opts) {
                        Ok(s) => s,
                        Err(e) => return format!("Engine error: {e}"),
                    };

                    let skel_tokens = budget::count_tokens(&skel, tokenizer);
                    let parse_errors = has_parse_errors(&source, backend.as_ref());

                    // Store in persistent cache
                    if let Some(cache) = skeleton_cache {
                        cache.store(file, &crate::cache::CacheEntry {
                            content_hash: cache::hash_content(&source),
                            mtime_secs: mtime,
                            original_tokens,
                            skeleton_tokens: skel_tokens,
                            has_parse_errors: parse_errors,
                            skeleton: skel.clone(),
                        });
                    }

                    (skel, skel_tokens, parse_errors)
                };

            let warning = if parse_errors {
                " | warning: parse errors detected"
            } else {
                ""
            };

            let compression = if original_tokens > 0 {
                ((1.0 - skeleton_tokens as f64 / original_tokens as f64) * 100.0) as u32
            } else {
                0
            };

            // Record savings for the dashboard
            if let Some(writer) = savings_writer.as_ref() {
                let saved_tokens = original_tokens.saturating_sub(skeleton_tokens);
                writer.record(SavingsRecord {
                    timestamp: OffsetDateTime::now_utc(),
                    file: file.to_string(),
                    language: lang.to_string(),
                    original_tokens,
                    skeleton_tokens,
                    saved_tokens,
                });
            }

            if record {
                if let Some(cache) = skeleton_cache {
                    cache.record_manifest_entry(file);
                }
            }

            format!(
                "[file: {file} | language: {lang} | original: {original_tokens} tokens | skeleton: {skeleton_tokens} tokens | compression: {compression}%{warning}]\n\n{skeleton}"
            )
        }
    }
}
