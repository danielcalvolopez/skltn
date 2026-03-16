use std::fs;
use std::path::Path;

fn create_test_repo(root: &Path) {
    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("main.rs"), "fn main() {}").unwrap();
    fs::write(src.join("lib.rs"), "pub fn hello() {}").unwrap();

    let backend = src.join("backend");
    fs::create_dir_all(&backend).unwrap();
    fs::write(backend.join("mod.rs"), "pub mod rust;").unwrap();
    fs::write(backend.join("rust.rs"), "pub struct RustBackend;").unwrap();

    // Unsupported file — should be omitted
    fs::write(src.join("README.md"), "# Hello").unwrap();

    let tests = root.join("tests");
    fs::create_dir_all(&tests).unwrap();
    fs::write(tests.join("integration.rs"), "#[test] fn it_works() {}").unwrap();
}

#[test]
fn test_basic_tree_output() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    let output = skltn_mcp::tools::list_repo_structure::build_tree(dir.path(), ".", None);
    assert!(output.contains("src/"));
    assert!(output.contains("main.rs"));
    assert!(output.contains("lib.rs"));
    assert!(output.contains("backend/"));
    assert!(output.contains("rust.rs"));
    assert!(output.contains("tests/"));
    assert!(output.contains("integration.rs"));
    // Unsupported files omitted
    assert!(!output.contains("README.md"));
    // File metadata present
    assert!(output.contains("bytes"));
    assert!(output.contains("rust"));
}

#[test]
fn test_max_depth_limits_traversal() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    let output = skltn_mcp::tools::list_repo_structure::build_tree(dir.path(), ".", Some(1));
    // Depth 1: src/ and tests/ visible, but not src/backend/
    assert!(output.contains("src/"));
    assert!(output.contains("main.rs"));
    assert!(!output.contains("backend/"));
}

#[test]
fn test_subdirectory_listing() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    let output = skltn_mcp::tools::list_repo_structure::build_tree(dir.path(), "src", None);
    assert!(output.contains("main.rs"));
    assert!(output.contains("backend/"));
    // Should NOT show tests/ (we're listing src/ only)
    assert!(!output.contains("integration.rs"));
}

#[test]
fn test_empty_directory_pruned() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let empty = root.join("empty_dir");
    fs::create_dir_all(&empty).unwrap();
    let docs = root.join("docs");
    fs::create_dir_all(&docs).unwrap();
    fs::write(docs.join("notes.md"), "# Notes").unwrap();
    fs::write(root.join("main.rs"), "fn main() {}").unwrap();

    let output = skltn_mcp::tools::list_repo_structure::build_tree(root, ".", None);
    assert!(!output.contains("empty_dir"));
    assert!(!output.contains("docs"));
    assert!(output.contains("main.rs"));
}

#[test]
fn test_python_files_detected() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("app.py"), "def main(): pass").unwrap();

    let output = skltn_mcp::tools::list_repo_structure::build_tree(root, ".", None);
    assert!(output.contains("app.py"));
    assert!(output.contains("python"));
}
