use std::fs;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

use skltn_mcp::cache::SkeletonCache;
use skltn_mcp::session::SessionTracker;

fn tokenizer() -> tiktoken_rs::CoreBPE {
    tiktoken_rs::cl100k_base().unwrap()
}

fn new_tracker() -> Arc<Mutex<SessionTracker>> {
    Arc::new(Mutex::new(SessionTracker::new()))
}

/// Helper: simulate session 1 by reading files, then create a new cache for session 2.
fn setup_two_sessions(root: &std::path::Path, files: &[(&str, &str)]) -> SkeletonCache {
    // Session 1: read all files
    {
        let cache = SkeletonCache::new(root).unwrap();
        let tok = tokenizer();
        for (name, _content) in files {
            skltn_mcp::tools::read_skeleton::read_skeleton_or_full(
                root, name, &tok, &new_tracker(), &None, Some(&cache), true,
            );
        }
        cache.force_flush_manifest();
    }
    // Session 2: new cache instance (rotates manifest)
    SkeletonCache::new(root).unwrap()
}

#[test]
fn test_restore_no_previous_session() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();
    let cache = SkeletonCache::new(dir.path()).unwrap();
    let tok = tokenizer();

    let output = skltn_mcp::tools::restore_session::restore_session(
        dir.path(), &tok, &cache, false, false,
    );
    assert!(output.contains("No previous session found"));
}

#[test]
fn test_restore_toc_mode_shows_files() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("lib.rs"), "pub fn hello() {}\n").unwrap();

    let cache = setup_two_sessions(root, &[("main.rs", ""), ("lib.rs", "")]);

    let output = skltn_mcp::tools::restore_session::restore_session(
        root, &tokenizer(), &cache, false, false,
    );
    assert!(output.contains("main.rs"));
    assert!(output.contains("lib.rs"));
    assert!(output.contains("unchanged"));
}

#[test]
fn test_restore_toc_detects_modified_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();

    let cache = setup_two_sessions(root, &[("main.rs", "")]);

    // Sleep to ensure mtime of the modified file is strictly after the manifest timestamp
    std::thread::sleep(std::time::Duration::from_secs(1));

    // Modify the file after session 1
    fs::write(root.join("main.rs"), "fn main() { println!(\"changed\"); }\n").unwrap();

    let output = skltn_mcp::tools::restore_session::restore_session(
        root, &tokenizer(), &cache, false, false,
    );
    assert!(output.contains("modified"));
}

#[test]
fn test_restore_toc_detects_removed_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();

    let cache = setup_two_sessions(root, &[("main.rs", "")]);

    // Remove the file
    fs::remove_file(root.join("main.rs")).unwrap();

    let output = skltn_mcp::tools::restore_session::restore_session(
        root, &tokenizer(), &cache, false, false,
    );
    assert!(output.contains("removed"));
}

#[test]
fn test_restore_load_mode_returns_content() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();

    let cache = setup_two_sessions(root, &[("main.rs", "")]);

    let output = skltn_mcp::tools::restore_session::restore_session(
        root, &tokenizer(), &cache, true, false,
    );
    // Should contain actual file content, not just TOC
    assert!(output.contains("fn main()"));
    assert!(output.contains("[file: main.rs"));
}

#[test]
fn test_restore_load_only_changed_skips_unchanged() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("lib.rs"), "pub fn hello() {}\n").unwrap();

    let cache = setup_two_sessions(root, &[("main.rs", ""), ("lib.rs", "")]);

    // Sleep to ensure mtime of the modified file is strictly after the manifest timestamp
    std::thread::sleep(std::time::Duration::from_secs(1));

    // Modify only main.rs
    fs::write(root.join("main.rs"), "fn main() { println!(\"changed\"); }\n").unwrap();

    let output = skltn_mcp::tools::restore_session::restore_session(
        root, &tokenizer(), &cache, true, true,
    );
    // Should contain main.rs content but not lib.rs content
    assert!(output.contains("fn main()"));
    assert!(!output.contains("pub fn hello()"));
}
