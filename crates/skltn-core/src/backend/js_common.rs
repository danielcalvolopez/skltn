use tree_sitter::Node;

/// Shared logic for JavaScript and TypeScript backends.
/// TS is a superset of JS, so most structural node classification is identical.
/// Is this node a structural node shared by both JS and TS?
pub fn is_structural_node_common(node: &Node) -> bool {
    matches!(
        node.kind(),
        "function_declaration"
            | "generator_function_declaration"
            | "method_definition"
            | "class_declaration"
            | "arrow_function"
            | "function"  // function expressions
            | "generator_function" // generator function expressions
    )
}

/// Is this a JSDoc-style doc comment?
pub fn is_doc_comment_common(node: &Node, source: &[u8]) -> bool {
    node.kind() == "comment"
        && node
            .utf8_text(source)
            .map(|t| t.starts_with("/**"))
            .unwrap_or(false)
}

/// Find the body node for a JS/TS structural node.
pub fn body_node_common<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    match node.kind() {
        // Leaf structural nodes
        "function_declaration"
        | "generator_function_declaration"
        | "method_definition"
        | "function"
        | "generator_function" => {
            node.child_by_field_name("body")
        }
        "arrow_function" => {
            // Only prune block-bodied arrows
            node.child_by_field_name("body")
                .filter(|body| body.kind() == "statement_block")
        }
        // Container structural node
        "class_declaration" => None,
        _ => None,
    }
}

/// Format the replacement for a brace-delimited body (JS/TS).
pub fn format_replacement_common(
    indent: &str,
    placeholder: &str,
    _line_count: usize,
    hidden_line_tag: &str,
) -> String {
    let inner_indent = format!("{}    ", indent);
    format!(
        "{{\n{}{} {}\n{}}}",
        inner_indent,
        placeholder,
        hidden_line_tag,
        indent,
    )
}
