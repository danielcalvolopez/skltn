use tree_sitter::{Node, Parser};

use crate::backend::LanguageBackend;
use crate::error::SkltnError;
use crate::options::SkeletonOptions;

/// A replacement to apply to the source text.
struct Replacement {
    start: usize,
    end: usize,
    text: String,
}

pub struct SkeletonEngine;

impl SkeletonEngine {
    /// Skeletonize source code using the given language backend.
    pub fn skeletonize(
        source: &str,
        backend: &dyn LanguageBackend,
        options: &SkeletonOptions,
    ) -> Result<String, SkltnError> {
        let mut parser = Parser::new();
        parser
            .set_language(&backend.language())
            .map_err(|e| SkltnError::ParseError(e.to_string()))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| SkltnError::ParseError("tree-sitter returned no tree".into()))?;

        let mut replacements = Vec::new();
        Self::walk_node(
            &tree.root_node(),
            source.as_bytes(),
            backend,
            options,
            0, // current depth
            &mut replacements,
        );

        // Apply replacements in reverse order to preserve byte offsets
        replacements.sort_by(|a, b| b.start.cmp(&a.start));

        let mut result = source.to_string();
        for rep in &replacements {
            result.replace_range(rep.start..rep.end, &rep.text);
        }

        Ok(result)
    }

    fn walk_node(
        node: &Node,
        source: &[u8],
        backend: &dyn LanguageBackend,
        options: &SkeletonOptions,
        depth: usize,
        replacements: &mut Vec<Replacement>,
    ) {
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if child.kind() == "ERROR" {
                // Emit verbatim — no replacement needed, just skip recursion
                continue;
            }

            if backend.is_structural_node(&child) {
                match backend.body_node(&child) {
                    Some(body) => {
                        // Leaf structural node — prune body
                        if let Some(max) = options.max_depth {
                            if depth >= max {
                                // At max depth: emit verbatim, don't recurse
                                continue;
                            }
                        }

                        let line_count =
                            body.end_position().row - body.start_position().row + 1;

                        // Calculate indentation: use the first line of the body
                        // to determine the indentation level
                        let body_start_line_start = source[..body.start_byte()]
                            .iter()
                            .rposition(|&b| b == b'\n')
                            .map(|p| p + 1)
                            .unwrap_or(0);
                        let indent_str: String = source[body_start_line_start..body.start_byte()]
                            .iter()
                            .take_while(|&&b| b == b' ' || b == b'\t')
                            .map(|&b| b as char)
                            .collect();

                        // Delegate formatting to the backend (handles brace vs indentation languages)
                        let replacement_text = backend.format_replacement(&indent_str, line_count, &body, source);

                        replacements.push(Replacement {
                            start: body.start_byte(),
                            end: body.end_byte(),
                            text: replacement_text,
                        });

                        // Do NOT recurse into this node's children — the body is being
                        // replaced, and any nested functions inside it are intentionally hidden.
                    }
                    None => {
                        // Container structural node (impl, class, module) or abstract method.
                        // Recurse into children without incrementing depth.
                        Self::walk_node(&child, source, backend, options, depth, replacements);
                    }
                }
            } else {
                // Non-structural node — emit verbatim, skip subtree.
                // No replacement needed, no recursion needed.
            }
        }
    }
}
