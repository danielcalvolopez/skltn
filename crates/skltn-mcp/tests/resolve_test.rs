use skltn_core::backend::rust::RustBackend;
use skltn_mcp::resolve::{resolve_symbol, ResolveResult};

#[test]
fn test_resolve_single_function() {
    let source = r#"
fn hello() {
    println!("hello");
}

fn world() {
    println!("world");
}
"#;
    let backend = RustBackend;
    let result = resolve_symbol(source, "hello", None, &backend);
    match result {
        ResolveResult::Found { match_info, .. } => {
            assert_eq!(match_info.name, "hello");
            assert!(match_info.parent_context.is_none());
        }
        other => panic!("Expected Found, got: {other:?}"),
    }
}

#[test]
fn test_resolve_method_in_impl() {
    let source = r#"
struct Foo;

impl Foo {
    fn bar(&self) {
        println!("bar");
    }
}
"#;
    let backend = RustBackend;
    let result = resolve_symbol(source, "bar", None, &backend);
    match result {
        ResolveResult::Found { match_info, .. } => {
            assert_eq!(match_info.name, "bar");
            assert_eq!(match_info.parent_context.as_deref(), Some("impl Foo"));
        }
        other => panic!("Expected Found, got: {other:?}"),
    }
}

#[test]
fn test_resolve_ambiguous_without_start_line() {
    let source = r#"
struct A;
struct B;

impl A {
    fn new() -> Self { A }
}

impl B {
    fn new() -> Self { B }
}
"#;
    let backend = RustBackend;
    let result = resolve_symbol(source, "new", None, &backend);
    match result {
        ResolveResult::Ambiguous { matches } => {
            assert_eq!(matches.len(), 2);
            assert_eq!(matches[0].parent_context.as_deref(), Some("impl A"));
            assert_eq!(matches[1].parent_context.as_deref(), Some("impl B"));
        }
        other => panic!("Expected Ambiguous, got: {other:?}"),
    }
}

#[test]
fn test_resolve_ambiguous_with_start_line() {
    let source = r#"
struct A;
struct B;

impl A {
    fn new() -> Self { A }
}

impl B {
    fn new() -> Self { B }
}
"#;
    let backend = RustBackend;
    let result = resolve_symbol(source, "new", Some(10), &backend);
    match result {
        ResolveResult::Found { match_info, .. } => {
            assert_eq!(match_info.parent_context.as_deref(), Some("impl B"));
        }
        other => panic!("Expected Found with start_line disambiguation, got: {other:?}"),
    }
}

#[test]
fn test_resolve_not_found() {
    let source = "fn hello() {}\n";
    let backend = RustBackend;
    let result = resolve_symbol(source, "nonexistent", None, &backend);
    match result {
        ResolveResult::NotFound => {}
        other => panic!("Expected NotFound, got: {other:?}"),
    }
}

#[test]
fn test_resolve_struct_data_node() {
    let source = r#"
pub struct UserProfile {
    pub name: String,
    pub age: u32,
}
"#;
    let backend = RustBackend;
    let result = resolve_symbol(source, "UserProfile", None, &backend);
    match result {
        ResolveResult::Found { match_info, .. } => {
            assert_eq!(match_info.name, "UserProfile");
        }
        other => panic!("Expected Found for struct, got: {other:?}"),
    }
}

#[test]
fn test_resolve_enum_data_node() {
    let source = r#"
pub enum Color {
    Red,
    Green,
    Blue,
}
"#;
    let backend = RustBackend;
    let result = resolve_symbol(source, "Color", None, &backend);
    match result {
        ResolveResult::Found { match_info, .. } => {
            assert_eq!(match_info.name, "Color");
        }
        other => panic!("Expected Found for enum, got: {other:?}"),
    }
}

#[test]
fn test_resolve_lines_are_1_indexed() {
    let source = "fn hello() {\n    println!(\"hello\");\n}\n";
    let backend = RustBackend;
    let result = resolve_symbol(source, "hello", None, &backend);
    match result {
        ResolveResult::Found { match_info, .. } => {
            assert_eq!(match_info.start_line, 1, "Lines should be 1-indexed");
            assert!(match_info.end_line >= 1);
        }
        other => panic!("Expected Found, got: {other:?}"),
    }
}

#[test]
fn test_resolve_includes_doc_comments() {
    let source = r#"/// This function greets someone.
/// It returns a greeting string.
pub fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}
"#;
    let backend = RustBackend;
    let result = resolve_symbol(source, "greet", None, &backend);
    match result {
        ResolveResult::Found { source_text, match_info } => {
            assert!(source_text.contains("/// This function greets someone."));
            assert!(source_text.contains("pub fn greet"));
            assert_eq!(match_info.start_line, 1);
        }
        other => panic!("Expected Found with doc comments, got: {other:?}"),
    }
}

#[test]
fn test_resolve_includes_attributes() {
    let source = r#"#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
}
"#;
    let backend = RustBackend;
    let result = resolve_symbol(source, "Config", None, &backend);
    match result {
        ResolveResult::Found { source_text, .. } => {
            assert!(source_text.contains("#[derive(Debug, Clone)]"));
            assert!(source_text.contains("pub struct Config"));
        }
        other => panic!("Expected Found with attributes, got: {other:?}"),
    }
}
