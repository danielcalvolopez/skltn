use std::fs;
use std::sync::{Arc, Mutex};

use skltn_mcp::session::SessionTracker;

fn tokenizer() -> tiktoken_rs::CoreBPE {
    tiktoken_rs::cl100k_base().unwrap()
}

fn new_tracker() -> Arc<Mutex<SessionTracker>> {
    Arc::new(Mutex::new(SessionTracker::new()))
}

#[test]
fn test_small_file_returned_full() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let source = "fn main() {\n    println!(\"hello\");\n}\n";
    fs::write(root.join("main.rs"), source).unwrap();

    let tok = tokenizer();
    let output =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "main.rs", &tok, &new_tracker(), &None, None, true);

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
    let output =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "big.rs", &tok, &new_tracker(), &None, None, true);

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
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(dir.path(), "nope.rs", &tok, &new_tracker(), &None, None, true);
    assert!(output.contains("File not found: nope.rs"));
}

#[test]
fn test_unsupported_language() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("readme.md"), "# Hello").unwrap();
    let tok = tokenizer();
    let output =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(dir.path(), "readme.md", &tok, &new_tracker(), &None, None, true);
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
        &new_tracker(),
        &None,
        None,
        true,
    );
    assert!(
        output.contains("Path is outside the repository root")
            || output.contains("File not found")
    );
}

#[test]
fn test_large_file_skeletonized_twice_without_full_serve() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Create a file large enough to be skeletonized (>2k tokens)
    let mut source = String::new();
    for i in 0..200 {
        source.push_str(&format!(
            "pub fn function_{i}(x: i32) -> i32 {{\n    let a = x + 1;\n    let b = a * 2;\n    let c = b - 3;\n    c + {i}\n}}\n\n"
        ));
    }
    fs::write(root.join("big.rs"), &source).unwrap();

    let tok = tokenizer();
    let tracker = new_tracker();

    // First read: should skeletonize (no cache hint)
    let output1 =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "big.rs", &tok, &tracker, &None, None, true);
    assert!(output1.contains("skeleton:"), "First read should skeletonize");

    // Second read: should ALSO skeletonize — skeletonized files are NOT
    // recorded in the tracker, so there's no RecentlyServed hint
    let output2 =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "big.rs", &tok, &tracker, &None, None, true);
    assert!(output2.contains("skeleton:"), "Second read should also skeletonize");
    assert!(!output2.contains("cache-aware"), "No cache-aware tag without prior full serve");
}

#[test]
fn test_small_file_served_full_then_large_version_served_full_cache_aware() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Start with a small file (under threshold)
    let small_source = "fn main() {\n    println!(\"hello\");\n}\n";
    fs::write(root.join("growing.rs"), small_source).unwrap();

    let tok = tokenizer();
    let tracker = new_tracker();

    // First read: small file returned full (under threshold), recorded in tracker
    let output1 =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "growing.rs", &tok, &tracker, &None, None, true);
    assert!(output1.contains("full file"));
    assert!(!output1.contains("cache-aware"));

    // Simulate the file growing (user adds code between reads)
    let mut large_source = String::new();
    for i in 0..200 {
        large_source.push_str(&format!(
            "pub fn function_{i}(x: i32) -> i32 {{\n    let a = x + 1;\n    let b = a * 2;\n    let c = b - 3;\n    c + {i}\n}}\n\n"
        ));
    }
    fs::write(root.join("growing.rs"), &large_source).unwrap();

    // Second read: file is now large, but tracker has a RecentlyServed hint
    // from the first read — serves full with cache-aware tag
    let output2 =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "growing.rs", &tok, &tracker, &None, None, true);
    assert!(
        output2.contains("full file (cache-aware)"),
        "Should serve full with cache-aware tag due to prior full serve"
    );
}

#[test]
fn test_small_file_not_tagged_cache_aware() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let source = "fn main() {\n    println!(\"hello\");\n}\n";
    fs::write(root.join("small.rs"), source).unwrap();

    let tok = tokenizer();
    let tracker = new_tracker();

    // First read: small file returned full (under threshold)
    let output1 =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "small.rs", &tok, &tracker, &None, None, true);
    assert!(output1.contains("full file"));
    assert!(!output1.contains("cache-aware"), "First read should not be cache-aware");

    // Second read: still under threshold, hint is RecentlyServed but the
    // cache-aware tag only appears when the hint CHANGED the decision.
    // Since this file is still small, it would be full anyway — no tag.
    let output2 =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "small.rs", &tok, &tracker, &None, None, true);
    assert!(output2.contains("full file"));
    assert!(!output2.contains("cache-aware"), "Small file should never get cache-aware tag");
}

#[test]
fn test_read_full_symbol_does_not_update_tracker() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Create a large file with a known symbol
    let mut source = String::new();
    source.push_str("pub fn target_symbol(x: i32) -> i32 {\n    let a = x + 1;\n    let b = a * 2;\n    b\n}\n\n");
    for i in 0..200 {
        source.push_str(&format!(
            "pub fn function_{i}(x: i32) -> i32 {{\n    let a = x + 1;\n    let b = a * 2;\n    let c = b - 3;\n    c + {i}\n}}\n\n"
        ));
    }
    fs::write(root.join("big.rs"), &source).unwrap();

    let tok = tokenizer();
    let tracker = new_tracker();

    // Call read_full_symbol — this should NOT update the tracker
    let _symbol_output =
        skltn_mcp::tools::read_full_symbol::read_full_symbol(root, "big.rs", "target_symbol", None, &tok, &None);

    // Now call read_skeleton — should still skeletonize (tracker not updated by read_full_symbol)
    let skeleton_output =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "big.rs", &tok, &tracker, &None, None, true);
    assert!(
        skeleton_output.contains("skeleton:"),
        "read_full_symbol should not cause read_skeleton to serve full"
    );
}
