pub mod list_repo_structure;
pub mod read_skeleton;

use skltn_core::backend::LanguageBackend;

pub fn backend_for_extension(ext: &str) -> Option<Box<dyn LanguageBackend>> {
    skltn_core::backend::backend_for_extension(ext)
}

pub fn language_name(ext: &str) -> &'static str {
    match ext {
        "rs" => "rust",
        "py" => "python",
        "ts" => "typescript",
        "js" => "javascript",
        _ => "unknown",
    }
}

pub fn has_parse_errors(source: &str, backend: &dyn LanguageBackend) -> bool {
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&backend.language()).is_err() {
        return true;
    }
    match parser.parse(source, None) {
        Some(tree) => has_error_nodes(tree.root_node()),
        None => true,
    }
}

fn has_error_nodes(node: tree_sitter::Node) -> bool {
    if node.is_error() || node.is_missing() {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_error_nodes(child) {
            return true;
        }
    }
    false
}
