pub mod python;
pub mod rust;

use tree_sitter::{Language, Node};

/// Trait that each supported language must implement.
/// The engine delegates all language-specific AST decisions to this trait.
///
/// Two categories of structural nodes exist:
/// - "Leaf" structural nodes (functions, methods) have a body to prune.
///   `body_node()` returns `Some(body)` for these.
/// - "Container" structural nodes (impl blocks, classes, modules) have no body
///   to prune, but the engine recurses into their children.
///   `body_node()` returns `None` for these.
pub trait LanguageBackend {
    /// Returns the tree-sitter Language grammar.
    fn language(&self) -> Language;

    /// File extensions this backend handles (e.g., &["rs"]).
    fn extensions(&self) -> &[&str];

    /// Is this AST node a structural node (leaf or container)?
    fn is_structural_node(&self, node: &Node) -> bool;

    /// Is this a doc comment node that should be preserved?
    /// Source bytes are needed because tree-sitter node kinds alone can't
    /// distinguish `///` doc comments from `//` regular comments.
    fn is_doc_comment(&self, node: &Node, source: &[u8]) -> bool;

    /// Given a structural node, return the child node representing the body.
    /// Returns Some(body) for leaf structural nodes (functions, methods).
    /// Returns None for container nodes (impl blocks, classes) and abstract methods.
    fn body_node<'a>(&self, node: &Node<'a>) -> Option<Node<'a>>;

    /// Returns the idiomatic placeholder for this language.
    /// e.g., "todo!()" for Rust, "pass" for Python.
    fn placeholder(&self) -> &str;

    /// Returns the formatted line-count tag comment.
    /// e.g., "// [skltn: 47 lines hidden]" for Rust.
    fn hidden_line_tag(&self, count: usize) -> String;

    /// Format the replacement text for a pruned body.
    /// This is language-specific because brace-delimited languages (Rust, JS, TS)
    /// need `{ placeholder }` while indentation-based languages (Python)
    /// need just the indented placeholder.
    /// `indent` is the whitespace string matching the body's indentation level.
    /// `body` and `source` are provided so backends can extract leading docstrings (Python).
    fn format_replacement(&self, indent: &str, line_count: usize, body: &Node, source: &[u8]) -> String;
}
