use tree_sitter::{Language, Node};

use super::LanguageBackend;

pub struct RustBackend;

impl LanguageBackend for RustBackend {
    fn language(&self) -> Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn extensions(&self) -> &[&str] {
        &["rs"]
    }

    fn is_structural_node(&self, node: &Node) -> bool {
        matches!(
            node.kind(),
            "function_item"
                | "impl_item"
                | "trait_item"
                | "mod_item"
                | "closure_expression"
        )
    }

    fn is_doc_comment(&self, node: &Node, source: &[u8]) -> bool {
        matches!(node.kind(), "line_comment" | "block_comment")
            && node
                .utf8_text(source)
                .map(|t| t.starts_with("///") || t.starts_with("//!") || t.starts_with("/**"))
                .unwrap_or(false)
    }

    fn body_node<'a>(&self, node: &Node<'a>) -> Option<Node<'a>> {
        match node.kind() {
            // Leaf structural nodes — have a body to prune
            "function_item" => node.child_by_field_name("body"),
            "closure_expression" => {
                // Only prune block-bodied closures
                node.child_by_field_name("body")
                    .filter(|body| body.kind() == "block")
            }
            // Container structural nodes — recurse, don't prune
            "impl_item" | "trait_item" | "mod_item" => None,
            _ => None,
        }
    }

    fn placeholder(&self) -> &str {
        "todo!()"
    }

    fn hidden_line_tag(&self, count: usize) -> String {
        format!("// [skltn: {} lines hidden]", count)
    }

    fn format_replacement(&self, indent: &str, line_count: usize, _body: &Node, _source: &[u8]) -> String {
        // For Rust, replace the entire block node: { placeholder // tag }
        let inner_indent = format!("{}    ", indent);
        format!(
            "{{\n{}{} {}\n{}}}",
            inner_indent,
            self.placeholder(),
            self.hidden_line_tag(line_count),
            indent,
        )
    }
}
