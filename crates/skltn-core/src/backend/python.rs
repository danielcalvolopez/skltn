use tree_sitter::{Language, Node};

use super::LanguageBackend;

pub struct PythonBackend;

impl PythonBackend {
    /// Extract the leading docstring from a Python function body block, if present.
    /// Returns the docstring text (including quotes) and its byte range.
    pub fn extract_docstring<'a>(body: &Node<'a>, source: &[u8]) -> Option<String> {
        // In Python's tree-sitter AST, the body is a `block` node.
        // A docstring is the first child if it's an `expression_statement`
        // containing a `string` node.
        let first_child = body.child(0)?;
        if first_child.kind() != "expression_statement" {
            return None;
        }
        let string_node = first_child.child(0)?;
        if string_node.kind() != "string" {
            return None;
        }
        let text = string_node.utf8_text(source).ok()?;
        // Only triple-quoted strings are docstrings
        if text.starts_with("\"\"\"") || text.starts_with("'''") {
            Some(first_child.utf8_text(source).ok()?.to_string())
        } else {
            None
        }
    }
}

impl LanguageBackend for PythonBackend {
    fn language(&self) -> Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn extensions(&self) -> &[&str] {
        &["py"]
    }

    fn is_structural_node(&self, node: &Node) -> bool {
        matches!(
            node.kind(),
            "function_definition" | "class_definition"
        )
    }

    fn is_doc_comment(&self, node: &Node, _source: &[u8]) -> bool {
        // Python standalone comments (# ...) are always preserved
        // as non-structural nodes (they pass through verbatim).
        // Docstrings inside function bodies are handled specially
        // by extract_docstring() in format_replacement().
        node.kind() == "comment"
    }

    fn body_node<'a>(&self, node: &Node<'a>) -> Option<Node<'a>> {
        match node.kind() {
            // Leaf structural node — has body to prune
            "function_definition" => node.child_by_field_name("body"),
            // Container structural node — recurse into children
            "class_definition" => None,
            _ => None,
        }
    }

    fn placeholder(&self) -> &str {
        "pass"
    }

    fn hidden_line_tag(&self, count: usize) -> String {
        format!("# [skltn: {} lines hidden]", count)
    }

    fn format_replacement(&self, indent: &str, line_count: usize, body: &Node, source: &[u8]) -> String {
        // Python has no braces — the body is an indented block.
        // Extract and preserve leading docstrings before replacing the body.
        let docstring = PythonBackend::extract_docstring(body, source);
        match docstring {
            Some(doc) => format!(
                "\n{}{}\n{}{} {}",
                indent, doc,
                indent, self.placeholder(), self.hidden_line_tag(line_count),
            ),
            None => format!(
                "\n{}{} {}",
                indent, self.placeholder(), self.hidden_line_tag(line_count),
            ),
        }
    }
}
