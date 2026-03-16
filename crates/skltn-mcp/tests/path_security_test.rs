use std::fs;

#[test]
fn test_resolve_valid_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let file = root.join("test.rs");
    fs::write(&file, "fn main() {}").unwrap();

    let result = skltn_mcp::resolve::resolve_safe_path(root, "test.rs");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), file.canonicalize().unwrap());
}

#[test]
fn test_resolve_subdirectory_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let sub = root.join("src");
    fs::create_dir(&sub).unwrap();
    let file = sub.join("lib.rs");
    fs::write(&file, "pub fn hello() {}").unwrap();

    let result = skltn_mcp::resolve::resolve_safe_path(root, "src/lib.rs");
    assert!(result.is_ok());
}

#[test]
fn test_reject_path_traversal() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let result = skltn_mcp::resolve::resolve_safe_path(root, "../../../etc/passwd");
    assert!(result.is_err());
    match result.unwrap_err() {
        skltn_mcp::error::McpError::PathOutsideRoot => {}
        other => panic!("Expected PathOutsideRoot, got: {other:?}"),
    }
}

#[test]
fn test_reject_nonexistent_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let result = skltn_mcp::resolve::resolve_safe_path(root, "nonexistent.rs");
    assert!(result.is_err());
}

#[test]
fn test_resolve_dot_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let result = skltn_mcp::resolve::resolve_safe_path(root, ".");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), root.canonicalize().unwrap());
}
