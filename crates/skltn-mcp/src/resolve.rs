use std::path::{Component, Path, PathBuf};

use skltn_core::backend::LanguageBackend;
use tree_sitter::{Node, Parser};

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

// ---------------------------------------------------------------------------
// Symbol resolution
// ---------------------------------------------------------------------------

/// Information about a matched symbol in the AST.
#[derive(Debug)]
pub struct MatchInfo {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub parent_context: Option<String>,
}

/// Result of attempting to resolve a symbol name within a source file.
#[derive(Debug)]
pub enum ResolveResult {
    /// Exactly one match was found (or disambiguation via `start_line` succeeded).
    Found {
        source_text: String,
        match_info: MatchInfo,
    },
    /// Multiple matches exist and no `start_line` was provided to disambiguate.
    Ambiguous {
        matches: Vec<MatchInfo>,
    },
    /// No symbol with the given name exists in the source.
    NotFound,
}

/// Data node kinds that are valid lookup targets per language.
fn is_data_node(kind: &str, lang_extensions: &[&str]) -> bool {
    let is_rust = lang_extensions.contains(&"rs");
    let is_typescript = lang_extensions.contains(&"ts");

    if is_rust {
        matches!(
            kind,
            "struct_item"
                | "enum_item"
                | "trait_item"
                | "type_item"
                | "const_item"
                | "static_item"
        )
    } else if is_typescript {
        matches!(
            kind,
            "interface_declaration" | "type_alias_declaration" | "enum_declaration"
        )
    } else {
        false
    }
}

/// Extract the name identifier from a node using the "name" field.
fn node_name(node: &Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .map(|n| n.utf8_text(source).unwrap_or("").to_string())
}

/// Extract parent context string for container nodes (impl blocks, classes, modules).
fn container_context(node: &Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "impl_item" => {
            let mut text = String::from("impl ");
            if let Some(trait_node) = node.child_by_field_name("trait") {
                let trait_name = trait_node.utf8_text(source).unwrap_or("");
                text.push_str(trait_name);
                text.push_str(" for ");
            }
            if let Some(type_node) = node.child_by_field_name("type") {
                let type_name = type_node.utf8_text(source).unwrap_or("");
                text.push_str(type_name);
            }
            Some(text)
        }
        "class_definition" | "class_declaration" => {
            node_name(node, source).map(|name| format!("class {name}"))
        }
        "module" => node_name(node, source).map(|name| format!("mod {name}")),
        _ => None,
    }
}

/// Look back at preceding siblings to find doc comments and attributes/decorators
/// that logically belong to the given node. Returns the byte offset where the
/// "extended" node begins (inclusive of doc comments and attributes).
fn extended_start_byte(node: &Node, source: &[u8], lang_extensions: &[&str]) -> usize {
    let is_python = lang_extensions.contains(&"py");
    let mut start = node.start_byte();
    let mut prev = node.prev_sibling();

    while let Some(sibling) = prev {
        let kind = sibling.kind();
        let is_doc_or_decorator = if is_python {
            kind == "decorator"
        } else {
            kind == "attribute_item"
                || kind == "decorator"
                || (kind == "line_comment" && {
                    let text = sibling.utf8_text(source).unwrap_or("");
                    text.starts_with("///") || text.starts_with("//!")
                })
                || (kind == "block_comment" && {
                    let text = sibling.utf8_text(source).unwrap_or("");
                    text.starts_with("/**")
                })
        };

        if is_doc_or_decorator {
            start = sibling.start_byte();
            prev = sibling.prev_sibling();
        } else {
            break;
        }
    }

    start
}

/// Mutable state accumulated during the AST walk.
struct WalkState<'a> {
    source_bytes: &'a [u8],
    source: &'a str,
    symbol: &'a str,
    lang_extensions: &'a [&'a str],
    backend: &'a dyn LanguageBackend,
    scope_stack: Vec<String>,
    matches: Vec<(MatchInfo, usize, usize)>,
}

impl WalkState<'_> {
    /// Recursively walk the AST collecting symbols that match `self.symbol`.
    fn walk_node(&mut self, node: Node) {
        let kind = node.kind();
        let is_structural = self.backend.is_structural_node(&node);
        let is_data = is_data_node(kind, self.lang_extensions);

        // Track container scope for parent_context reporting.
        let pushed_scope = if let Some(ctx) = container_context(&node, self.source_bytes) {
            self.scope_stack.push(ctx);
            true
        } else {
            false
        };

        if is_structural || is_data {
            if let Some(name) = node_name(&node, self.source_bytes) {
                if name == self.symbol {
                    let ext_start =
                        extended_start_byte(&node, self.source_bytes, self.lang_extensions);
                    let end = node.end_byte();
                    let ext_start_line =
                        self.source[..ext_start].matches('\n').count() + 1;
                    let info = MatchInfo {
                        name,
                        start_line: ext_start_line,
                        end_line: node.end_position().row + 1,
                        parent_context: self.scope_stack.last().cloned(),
                    };
                    self.matches.push((info, ext_start, end));
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_node(child);
        }

        if pushed_scope {
            self.scope_stack.pop();
        }
    }
}

/// Resolve a symbol name within source code, returning the matched source text
/// and metadata. When multiple matches exist and `start_line` is provided, the
/// closest match by line number is selected.
pub fn resolve_symbol(
    source: &str,
    symbol: &str,
    start_line: Option<usize>,
    backend: &dyn LanguageBackend,
) -> ResolveResult {
    let mut parser = Parser::new();
    if parser.set_language(&backend.language()).is_err() {
        return ResolveResult::NotFound;
    }

    let tree = match parser.parse(source, None) {
        Some(tree) => tree,
        None => return ResolveResult::NotFound,
    };

    let mut state = WalkState {
        source_bytes: source.as_bytes(),
        source,
        symbol,
        lang_extensions: backend.extensions(),
        backend,
        scope_stack: Vec::new(),
        matches: Vec::new(),
    };

    state.walk_node(tree.root_node());

    let mut matches = state.matches;

    match matches.len() {
        0 => ResolveResult::NotFound,
        1 => {
            let (info, ext_start, end) = matches.remove(0);
            let source_text = source[ext_start..end].to_string();
            ResolveResult::Found {
                source_text,
                match_info: info,
            }
        }
        _ => {
            if let Some(target_line) = start_line {
                // Disambiguate by choosing the match closest to target_line.
                let closest_idx = matches
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, (info, _, _))| {
                        (info.start_line as isize - target_line as isize).unsigned_abs()
                    })
                    .map(|(idx, _)| idx)
                    .expect("matches is non-empty");

                let (info, ext_start, end) = matches.remove(closest_idx);
                let source_text = source[ext_start..end].to_string();
                ResolveResult::Found {
                    source_text,
                    match_info: info,
                }
            } else {
                let match_infos = matches.into_iter().map(|(info, _, _)| info).collect();
                ResolveResult::Ambiguous {
                    matches: match_infos,
                }
            }
        }
    }
}
