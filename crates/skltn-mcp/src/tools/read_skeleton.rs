use std::path::Path;

use tiktoken_rs::CoreBPE;

use skltn_core::engine::SkeletonEngine;
use skltn_core::options::SkeletonOptions;

use crate::budget::{self, BudgetDecision};
use crate::error::McpError;
use crate::resolve::resolve_safe_path;

use super::{backend_for_extension, has_parse_errors, language_name};

/// Maximum file size we will attempt to read (10 MB).
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Read a source file, returning either its full content (if small) or a
/// skeletonized summary (if large). The decision is made by the budget guard
/// in [`crate::budget::should_skeletonize`].
///
/// The returned string includes a metadata header line followed by the content.
/// Error cases return a human-readable error message string.
pub fn read_skeleton_or_full(root: &Path, file: &str, tokenizer: &CoreBPE) -> String {
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

    // Check for parse errors once (used in both branches)
    let warning = if has_parse_errors(&source, backend.as_ref()) {
        " | warning: parse errors detected"
    } else {
        ""
    };

    match budget::should_skeletonize(&source, tokenizer) {
        BudgetDecision::ReturnFull { original_tokens } => {
            format!(
                "[file: {file} | language: {lang} | tokens: {original_tokens} | full file{warning}]\n\n{source}"
            )
        }
        BudgetDecision::Skeletonize { original_tokens } => {
            let opts = SkeletonOptions::default();
            let skeleton = match SkeletonEngine::skeletonize(&source, backend.as_ref(), &opts) {
                Ok(s) => s,
                Err(e) => return format!("Engine error: {e}"),
            };

            let skeleton_tokens = budget::count_tokens(&skeleton, tokenizer);
            let compression = if original_tokens > 0 {
                ((1.0 - skeleton_tokens as f64 / original_tokens as f64) * 100.0) as u32
            } else {
                0
            };

            format!(
                "[file: {file} | language: {lang} | original: {original_tokens} tokens | skeleton: {skeleton_tokens} tokens | compression: {compression}%{warning}]\n\n{skeleton}"
            )
        }
    }
}
