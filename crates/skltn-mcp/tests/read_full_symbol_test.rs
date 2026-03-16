use std::fs;

fn tokenizer() -> tiktoken_rs::CoreBPE {
    tiktoken_rs::cl100k_base().unwrap()
}

#[test]
fn test_single_match_returns_full_source() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let source = r#"pub fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}

pub fn farewell() -> &'static str {
    "Goodbye!"
}
"#;
    fs::write(root.join("lib.rs"), source).unwrap();

    let tok = tokenizer();
    let output = skltn_mcp::tools::read_full_symbol::read_full_symbol(root, "lib.rs", "greet", None, &tok);
    assert!(output.contains("[symbol: greet"));
    assert!(output.contains("pub fn greet(name: &str) -> String"));
    assert!(output.contains("format!(\"Hello, {name}!\")"));
}

#[test]
fn test_ambiguous_returns_list() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let source = r#"struct A;
struct B;

impl A {
    pub fn new() -> Self { A }
}

impl B {
    pub fn new() -> Self { B }
}
"#;
    fs::write(root.join("lib.rs"), source).unwrap();

    let tok = tokenizer();
    let output = skltn_mcp::tools::read_full_symbol::read_full_symbol(root, "lib.rs", "new", None, &tok);
    assert!(output.contains("Multiple matches for 'new'"));
    assert!(output.contains("impl A"));
    assert!(output.contains("impl B"));
    assert!(output.contains("start_line"));
}

#[test]
fn test_start_line_disambiguates() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let source = r#"struct A;
struct B;

impl A {
    pub fn new() -> Self { A }
}

impl B {
    pub fn new() -> Self { B }
}
"#;
    fs::write(root.join("lib.rs"), source).unwrap();

    let tok = tokenizer();
    let output = skltn_mcp::tools::read_full_symbol::read_full_symbol(root, "lib.rs", "new", Some(9), &tok);
    assert!(output.contains("[symbol: new"));
    assert!(output.contains("impl B"));
}

#[test]
fn test_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("lib.rs"), "fn hello() {}").unwrap();

    let tok = tokenizer();
    let output = skltn_mcp::tools::read_full_symbol::read_full_symbol(root, "lib.rs", "nonexistent", None, &tok);
    assert!(output.contains("Symbol 'nonexistent' not found in lib.rs"));
}

#[test]
fn test_file_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let tok = tokenizer();
    let output = skltn_mcp::tools::read_full_symbol::read_full_symbol(dir.path(), "nope.rs", "foo", None, &tok);
    assert!(output.contains("File not found: nope.rs"));
}

#[test]
fn test_data_node_struct_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let source = r#"pub struct Config {
    pub host: String,
    pub port: u16,
}
"#;
    fs::write(root.join("config.rs"), source).unwrap();

    let tok = tokenizer();
    let output = skltn_mcp::tools::read_full_symbol::read_full_symbol(root, "config.rs", "Config", None, &tok);
    assert!(output.contains("[symbol: Config"));
    assert!(output.contains("pub host: String"));
}
