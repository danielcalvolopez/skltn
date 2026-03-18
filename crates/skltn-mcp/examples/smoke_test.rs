use std::path::Path;
use std::sync::{Arc, Mutex};

use skltn_mcp::cache::SkeletonCache;
use skltn_mcp::session::SessionTracker;

fn main() {
    let root = Path::new(".");
    let root = root.canonicalize().unwrap();
    let tokenizer = tiktoken_rs::cl100k_base().unwrap();
    let tracker = Arc::new(Mutex::new(SessionTracker::new()));

    println!("=== SMOKE TEST: skltn-mcp tools against the skltn codebase ===\n");

    // --- Test 1: list_repo_structure ---
    println!("--- TEST 1: list_repo_structure (root, no depth limit) ---");
    let tree = skltn_mcp::tools::list_repo_structure::build_tree(&root, ".", None);
    println!("{}", tree);
    assert!(!tree.is_empty(), "Tree should not be empty");
    assert!(tree.contains("crates/"), "Should contain crates/");
    println!("PASS: list_repo_structure works\n");

    // --- Test 2: list_repo_structure with max_depth ---
    println!("--- TEST 2: list_repo_structure (max_depth=1) ---");
    let tree_shallow = skltn_mcp::tools::list_repo_structure::build_tree(&root, ".", Some(1));
    println!("{}", tree_shallow);
    println!("PASS: list_repo_structure with max_depth works\n");

    // --- Test 3: list_repo_structure subdirectory ---
    println!("--- TEST 3: list_repo_structure (crates/skltn-core/src) ---");
    let tree_sub = skltn_mcp::tools::list_repo_structure::build_tree(&root, "crates/skltn-core/src", None);
    println!("{}", tree_sub);
    assert!(tree_sub.contains("lib.rs"), "Should contain lib.rs");
    println!("PASS: subdirectory listing works\n");

    // --- Test 4: read_skeleton on a small file ---
    println!("--- TEST 4: read_skeleton (small file: crates/skltn-core/src/options.rs) ---");
    let result = skltn_mcp::tools::read_skeleton::read_skeleton_or_full(
        &root,
        "crates/skltn-core/src/options.rs",
        &tokenizer,
        &tracker,
        &None,
        None,
        true,
    );
    println!("{}\n", result);
    assert!(
        result.contains("full file"),
        "Small file should be returned in full"
    );
    println!("PASS: small file returned in full\n");

    // --- Test 5: read_skeleton on a larger file ---
    println!("--- TEST 5: read_skeleton (larger file: crates/skltn-mcp/src/resolve.rs) ---");
    let result = skltn_mcp::tools::read_skeleton::read_skeleton_or_full(
        &root,
        "crates/skltn-mcp/src/resolve.rs",
        &tokenizer,
        &tracker,
        &None,
        None,
        true,
    );
    let first_line = result.lines().next().unwrap_or("");
    println!("{}", first_line);
    if result.contains("skeleton:") {
        println!("(skeletonized — body replaced with placeholders)");
    } else {
        println!("(returned in full — under 2k tokens)");
    }
    println!("PASS: read_skeleton works\n");

    // --- Test 6: read_full_symbol - single match ---
    println!("--- TEST 6: read_full_symbol ('resolve_safe_path' in resolve.rs) ---");
    let result = skltn_mcp::tools::read_full_symbol::read_full_symbol(
        &root,
        "crates/skltn-mcp/src/resolve.rs",
        "resolve_safe_path",
        None,
        &tokenizer,
        &None,
    );
    let first_line = result.lines().next().unwrap_or("");
    println!("{}", first_line);
    assert!(
        result.contains("[symbol: resolve_safe_path"),
        "Should find the symbol"
    );
    assert!(
        result.contains("fn resolve_safe_path"),
        "Should contain the function"
    );
    println!("PASS: single symbol match works\n");

    // --- Test 7: read_full_symbol - data node (struct) ---
    println!("--- TEST 7: read_full_symbol (struct 'MatchInfo' in resolve.rs) ---");
    let result = skltn_mcp::tools::read_full_symbol::read_full_symbol(
        &root,
        "crates/skltn-mcp/src/resolve.rs",
        "MatchInfo",
        None,
        &tokenizer,
        &None,
    );
    let first_line = result.lines().next().unwrap_or("");
    println!("{}", first_line);
    assert!(
        result.contains("[symbol: MatchInfo"),
        "Should find the struct"
    );
    println!("PASS: data node (struct) resolution works\n");

    // --- Test 8: read_full_symbol on skltn-core ---
    println!("--- TEST 8: read_full_symbol ('backend_for_extension' in backend/mod.rs) ---");
    let result = skltn_mcp::tools::read_full_symbol::read_full_symbol(
        &root,
        "crates/skltn-core/src/backend/mod.rs",
        "backend_for_extension",
        None,
        &tokenizer,
        &None,
    );
    let first_line = result.lines().next().unwrap_or("");
    println!("{}", first_line);
    assert!(
        result.contains("[symbol: backend_for_extension"),
        "Should find the symbol"
    );
    println!("PASS: symbol resolution on skltn-core works\n");

    // --- Test 9: Error - file not found ---
    println!("--- TEST 9: Error - file not found ---");
    let result = skltn_mcp::tools::read_skeleton::read_skeleton_or_full(
        &root,
        "nonexistent.rs",
        &tokenizer,
        &tracker,
        &None,
        None,
        true,
    );
    println!("{}", result);
    assert!(
        result.contains("File not found"),
        "Should report file not found"
    );
    println!("PASS\n");

    // --- Test 10: Error - unsupported language ---
    println!("--- TEST 10: Error - unsupported language ---");
    let result =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(&root, "CLAUDE.md", &tokenizer, &tracker, &None, None, true);
    println!("{}", result);
    assert!(
        result.contains("Unsupported language"),
        "Should report unsupported language"
    );
    println!("PASS\n");

    // --- Test 11: Error - path traversal ---
    println!("--- TEST 11: Error - path traversal ---");
    let result = skltn_mcp::tools::read_skeleton::read_skeleton_or_full(
        &root,
        "../../../etc/passwd",
        &tokenizer,
        &tracker,
        &None,
        None,
        true,
    );
    println!("{}", result);
    assert!(
        result.contains("outside") || result.contains("not found"),
        "Should block path traversal"
    );
    println!("PASS\n");

    // --- Test 12: Error - symbol not found ---
    println!("--- TEST 12: Error - symbol not found ---");
    let result = skltn_mcp::tools::read_full_symbol::read_full_symbol(
        &root,
        "crates/skltn-mcp/src/resolve.rs",
        "nonexistent_xyz",
        None,
        &tokenizer,
        &None,
    );
    println!("{}", result);
    assert!(result.contains("not found"), "Should report symbol not found");
    println!("PASS\n");

    // --- Test 13: Cache-aware read_skeleton ---
    println!("--- TEST 13: cache-aware read_skeleton (resolve.rs read twice) ---");
    // Use a fresh tracker to isolate this test
    let cache_tracker = Arc::new(Mutex::new(SessionTracker::new()));
    let target_file = "crates/skltn-mcp/src/resolve.rs";

    // First read: should skeletonize (>2k tokens, no prior serve)
    let first_read = skltn_mcp::tools::read_skeleton::read_skeleton_or_full(
        &root,
        target_file,
        &tokenizer,
        &cache_tracker,
        &None,
        None,
        true,
    );
    let first_header = first_read.lines().next().unwrap_or("");
    println!("1st read: {}", first_header);
    assert!(
        first_read.contains("skeleton:"),
        "First read should skeletonize a >2k token file"
    );
    assert!(
        !first_read.contains("cache-aware"),
        "First read should not be cache-aware"
    );

    // Manually seed the tracker (simulates a prior full serve, e.g. file was
    // small on a previous read or served via a different path)
    let resolved = root.join(target_file).canonicalize().unwrap();
    cache_tracker.lock().unwrap().record_full(&resolved);

    // Second read: same file, but tracker now has a RecentlyServed hint —
    // should serve full with (cache-aware) tag
    let second_read = skltn_mcp::tools::read_skeleton::read_skeleton_or_full(
        &root,
        target_file,
        &tokenizer,
        &cache_tracker,
        &None,
        None,
        true,
    );
    let second_header = second_read.lines().next().unwrap_or("");
    println!("2nd read: {}", second_header);
    assert!(
        second_read.contains("full file (cache-aware)"),
        "Second read should serve full with cache-aware tag"
    );
    println!("PASS: cache-aware serving works\n");

    // --- Test 14: restore_session with no previous session ---
    println!("--- TEST 14: restore_session (no previous session) ---");
    {
        let cache = SkeletonCache::new(&root).expect("cache should init");
        let result = skltn_mcp::tools::restore_session::restore_session(
            &root, &tokenizer, &cache, false, false,
        );
        println!("{}", result);
        // On first run there's no previous manifest; on subsequent runs there will be one.
        // Either outcome is valid for a smoke test — just verify it doesn't panic.
        println!("PASS: restore_session handles current state without panic\n");
    }

    // --- Test 15: restore_session after manifest recording ---
    println!("--- TEST 15: restore_session (simulate session with manifest) ---");
    {
        // Session 1: read some files with manifest recording
        let cache1 = SkeletonCache::new(&root).expect("cache should init");
        let s1_tracker = Arc::new(Mutex::new(SessionTracker::new()));
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(
            &root,
            "crates/skltn-core/src/options.rs",
            &tokenizer,
            &s1_tracker,
            &None,
            Some(&cache1),
            true,
        );
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(
            &root,
            "crates/skltn-mcp/src/resolve.rs",
            &tokenizer,
            &s1_tracker,
            &None,
            Some(&cache1),
            true,
        );
        cache1.force_flush_manifest();

        // Verify manifest was written
        let manifest = cache1.load_current_manifest().expect("manifest should exist after flush");
        assert!(
            manifest.files.contains(&"crates/skltn-core/src/options.rs".to_string()),
            "Manifest should contain options.rs"
        );
        assert!(
            manifest.files.contains(&"crates/skltn-mcp/src/resolve.rs".to_string()),
            "Manifest should contain resolve.rs"
        );
        println!("Session 1: recorded {} files in manifest", manifest.files.len());

        // Session 2: restore_session should see session 1's files
        let cache2 = SkeletonCache::new(&root).expect("cache should init");

        // TOC mode
        let toc = skltn_mcp::tools::restore_session::restore_session(
            &root, &tokenizer, &cache2, false, false,
        );
        println!("TOC output:\n{}", toc);
        assert!(toc.contains("options.rs"), "TOC should list options.rs");
        assert!(toc.contains("resolve.rs"), "TOC should list resolve.rs");
        assert!(toc.contains("unchanged"), "Files should be unchanged");
        println!("PASS: TOC mode shows previous session files\n");

        // Load mode
        let loaded = skltn_mcp::tools::restore_session::restore_session(
            &root, &tokenizer, &cache2, true, false,
        );
        let first_line = loaded.lines().next().unwrap_or("");
        println!("Load output (first line): {}", first_line);
        assert!(
            loaded.contains("[file: crates/skltn-core/src/options.rs"),
            "Load mode should contain file header for options.rs"
        );
        println!("PASS: load mode returns file content from previous session\n");
    }

    println!("=== ALL 15 SMOKE TESTS PASSED ===");
}
