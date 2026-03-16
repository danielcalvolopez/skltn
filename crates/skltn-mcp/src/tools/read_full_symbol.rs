use std::path::Path;

use tiktoken_rs::CoreBPE;

use crate::budget;
use crate::error::McpError;
use crate::resolve::{resolve_safe_path, resolve_symbol, ResolveResult};

use super::{backend_for_extension, has_parse_errors};

/// Maximum file size we will attempt to read (10 MB).
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Read the full source text of a specific symbol (function, struct, impl block, etc.)
/// from a file, using AST-based symbol resolution.
///
/// When multiple symbols share the same name, the caller can supply `start_line`
/// to disambiguate. Without it, the function returns a listing of all matches
/// so the caller can re-invoke with the correct line number.
///
/// Error cases return a human-readable error message string.
pub fn read_full_symbol(
    root: &Path,
    file: &str,
    symbol: &str,
    start_line: Option<usize>,
    tokenizer: &CoreBPE,
) -> String {
    // Resolve path
    let path = match resolve_safe_path(root, file) {
        Ok(p) => p,
        Err(e) => return e.to_string(),
    };

    if !path.is_file() {
        return McpError::FileNotFound(file.to_string()).to_string();
    }

    // File size limit
    if let Ok(metadata) = std::fs::metadata(&path) {
        if metadata.len() > MAX_FILE_SIZE {
            return format!(
                "File too large: {} ({} bytes, limit is 10 MB)",
                file,
                metadata.len()
            );
        }
    }

    // Detect language
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let backend = match backend_for_extension(ext) {
        Some(b) => b,
        None => return McpError::UnsupportedLanguage(file.to_string()).to_string(),
    };

    // Read file
    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return McpError::FileNotFound(file.to_string()).to_string(),
    };

    let parse_warning = if has_parse_errors(&source, backend.as_ref()) {
        " | warning: parse errors detected"
    } else {
        ""
    };

    // Resolve symbol
    match resolve_symbol(&source, symbol, start_line, backend.as_ref()) {
        ResolveResult::Found {
            source_text,
            match_info,
        } => {
            let tokens = budget::count_tokens(&source_text, tokenizer);
            let context_suffix = match &match_info.parent_context {
                Some(ctx) => format!(" | context: {ctx}"),
                None => String::new(),
            };
            format!(
                "[symbol: {} | file: {} | lines: {}-{} | {} tokens{}{}]\n\n{}",
                match_info.name,
                file,
                match_info.start_line,
                match_info.end_line,
                tokens,
                context_suffix,
                parse_warning,
                source_text,
            )
        }
        ResolveResult::Ambiguous { matches } => {
            let mut result = format!("Multiple matches for '{symbol}':\n");
            for m in &matches {
                let context = m.parent_context.as_deref().unwrap_or("top-level");
                result.push_str(&format!(
                    "  - {} (start_line: {}, lines: {}-{}) in {}\n",
                    m.name, m.start_line, m.start_line, m.end_line, context,
                ));
            }
            result.push_str("\nPlease re-call with start_line to select one.");
            result
        }
        ResolveResult::NotFound => McpError::SymbolNotFound {
            name: symbol.to_string(),
            file: file.to_string(),
        }
        .to_string(),
    }
}
