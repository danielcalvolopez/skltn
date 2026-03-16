use skltn_core::backend::LanguageBackend;
use skltn_core::options::SkeletonOptions;

pub fn default_opts() -> SkeletonOptions {
    SkeletonOptions::default()
}

/// Recursively check if any node in the tree is an ERROR or MISSING node.
pub fn has_error_nodes(node: &tree_sitter::Node) -> bool {
    if node.is_error() || node.is_missing() {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_error_nodes(&child) {
            return true;
        }
    }
    false
}

/// Assert that the skeleton output is syntactically valid by re-parsing it.
pub fn assert_valid_syntax(skeleton: &str, backend: &dyn LanguageBackend) {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&backend.language()).unwrap();
    let tree = parser.parse(skeleton, None).unwrap();
    assert!(
        !has_error_nodes(&tree.root_node()),
        "Skeleton output has syntax errors:\n{}",
        skeleton
    );
}
