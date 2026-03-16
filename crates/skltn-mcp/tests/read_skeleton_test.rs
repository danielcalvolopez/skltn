use std::fs;

fn tokenizer() -> tiktoken_rs::CoreBPE {
    tiktoken_rs::cl100k_base().unwrap()
}

#[test]
fn test_small_file_returned_full() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let source = "fn main() {\n    println!(\"hello\");\n}\n";
    fs::write(root.join("main.rs"), source).unwrap();

    let tok = tokenizer();
    let output = skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "main.rs", &tok);

    assert!(output.contains("[file: main.rs"));
    assert!(output.contains("full file"));
    assert!(output.contains("fn main()"));
}

#[test]
fn test_large_file_returned_skeletonized() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let mut source = String::new();
    for i in 0..200 {
        source.push_str(&format!(
            "pub fn function_{i}(x: i32) -> i32 {{\n    let a = x + 1;\n    let b = a * 2;\n    let c = b - 3;\n    c + {i}\n}}\n\n"
        ));
    }
    fs::write(root.join("big.rs"), &source).unwrap();

    let tok = tokenizer();
    let output = skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "big.rs", &tok);

    assert!(output.contains("[file: big.rs"));
    assert!(output.contains("skeleton:"));
    assert!(output.contains("compression:"));
    assert!(output.contains("todo!()"));
}

#[test]
fn test_file_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let tok = tokenizer();
    let output =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(dir.path(), "nope.rs", &tok);
    assert!(output.contains("File not found: nope.rs"));
}

#[test]
fn test_unsupported_language() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("readme.md"), "# Hello").unwrap();
    let tok = tokenizer();
    let output =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(dir.path(), "readme.md", &tok);
    assert!(output.contains("Unsupported language"));
}

#[test]
fn test_path_traversal_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let tok = tokenizer();
    let output = skltn_mcp::tools::read_skeleton::read_skeleton_or_full(
        dir.path(),
        "../../../etc/passwd",
        &tok,
    );
    assert!(
        output.contains("Path is outside the repository root")
            || output.contains("File not found")
    );
}
