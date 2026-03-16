use tree_sitter::{Language, Node};

use super::js_common;
use super::LanguageBackend;

pub struct TypeScriptBackend;

impl LanguageBackend for TypeScriptBackend {
    fn language(&self) -> Language {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }

    fn extensions(&self) -> &[&str] {
        &["ts"]
    }

    fn is_structural_node(&self, node: &Node) -> bool {
        // All JS structural nodes + TS-specific abstract classes
        js_common::is_structural_node_common(node)
            || matches!(node.kind(), "abstract_class_declaration")
    }

    fn is_doc_comment(&self, node: &Node, source: &[u8]) -> bool {
        js_common::is_doc_comment_common(node, source)
    }

    fn body_node<'a>(&self, node: &Node<'a>) -> Option<Node<'a>> {
        match node.kind() {
            // TS-specific: abstract classes are containers
            "abstract_class_declaration" => None,
            // Delegate everything else to shared JS logic
            _ => js_common::body_node_common(node),
        }
    }

    fn placeholder(&self) -> &str {
        "throw new Error(\"not implemented\")"
    }

    fn hidden_line_tag(&self, count: usize) -> String {
        format!("// [skltn: {} lines hidden]", count)
    }

    fn format_replacement(&self, indent: &str, line_count: usize, _body: &Node, _source: &[u8]) -> String {
        js_common::format_replacement_common(indent, self.placeholder(), line_count, &self.hidden_line_tag(line_count))
    }
}
