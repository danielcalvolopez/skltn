# Cache-Aware Budget Guard Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the MCP server's Budget Guard cache-aware so it serves files full (instead of skeletonizing) when they've been served full previously in the session, avoiding the cost penalty of breaking prompt cache locality.

**Architecture:** Add a `CacheHint` enum and `SessionTracker` (in-memory `HashMap<PathBuf, Instant>`) to `skltn-mcp`. The `should_skeletonize()` function gains a `hint` parameter. `read_skeleton` queries the tracker before deciding, and records files served full. `read_full_symbol` and `list_repo_structure` are unchanged.

**Tech Stack:** Rust, `std::collections::HashMap`, `std::time::Instant`, existing `tiktoken-rs` + `skltn-core` dependencies. No new crate dependencies.

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `crates/skltn-mcp/src/budget.rs` | Modify | Add `CacheHint` enum, update `should_skeletonize()` signature and logic |
| `crates/skltn-mcp/src/session.rs` | Create | `SessionTracker` struct — `HashMap<PathBuf, Instant>` with `record_full()` and `hint_for()` |
| `crates/skltn-mcp/src/lib.rs` | Modify | Add `pub mod session;` |
| `crates/skltn-mcp/src/tools/mod.rs` | Modify | Add `SessionTracker` field to `SkltnServer`, pass to `read_skeleton` |
| `crates/skltn-mcp/src/tools/read_skeleton.rs` | Modify | Accept `SessionTracker`, query hint, record full serves, update metadata header |
| `crates/skltn-mcp/tests/budget_test.rs` | Modify | Update tests for new `should_skeletonize()` signature with `CacheHint` |
| `crates/skltn-mcp/tests/session_test.rs` | Create | Tests for `SessionTracker` |
| `crates/skltn-mcp/tests/read_skeleton_test.rs` | Modify | Add cache-aware integration tests |

---

## Chunk 1: CacheHint Enum and Updated Budget Guard

### Task 1: Add CacheHint enum and update should_skeletonize

**Files:**
- Modify: `crates/skltn-mcp/src/budget.rs`
- Modify: `crates/skltn-mcp/tests/budget_test.rs`

- [ ] **Step 1: Write failing tests for CacheHint-aware budget decisions**

Add these tests to `crates/skltn-mcp/tests/budget_test.rs`:

```rust
use skltn_mcp::budget::CacheHint;

#[test]
fn test_unknown_hint_small_file_returns_full() {
    let source = "fn main() {\n    println!(\"hello\");\n}\n";
    let tokenizer = tokenizer();
    let decision = skltn_mcp::budget::should_skeletonize(source, &tokenizer, CacheHint::Unknown);
    match decision {
        skltn_mcp::budget::BudgetDecision::ReturnFull { original_tokens } => {
            assert!(original_tokens <= 2000);
            assert!(original_tokens > 0);
        }
        _ => panic!("Expected ReturnFull for small file with Unknown hint"),
    }
}

#[test]
fn test_unknown_hint_large_file_returns_skeletonize() {
    let mut source = String::new();
    for i in 0..500 {
        source.push_str(&format!(
            "fn function_{i}(arg: i32) -> i32 {{\n    arg + {i}\n}}\n\n"
        ));
    }
    let tokenizer = tokenizer();
    let decision = skltn_mcp::budget::should_skeletonize(&source, &tokenizer, CacheHint::Unknown);
    match decision {
        skltn_mcp::budget::BudgetDecision::Skeletonize { original_tokens } => {
            assert!(original_tokens > 2000);
        }
        _ => panic!("Expected Skeletonize for large file with Unknown hint"),
    }
}

#[test]
fn test_recently_served_hint_large_file_returns_full() {
    let mut source = String::new();
    for i in 0..500 {
        source.push_str(&format!(
            "fn function_{i}(arg: i32) -> i32 {{\n    arg + {i}\n}}\n\n"
        ));
    }
    let tokenizer = tokenizer();
    let decision =
        skltn_mcp::budget::should_skeletonize(&source, &tokenizer, CacheHint::RecentlyServed);
    match decision {
        skltn_mcp::budget::BudgetDecision::ReturnFull { original_tokens } => {
            assert!(original_tokens > 2000);
        }
        _ => panic!("Expected ReturnFull for large file with RecentlyServed hint"),
    }
}

#[test]
fn test_cache_confirmed_hint_large_file_returns_full() {
    let mut source = String::new();
    for i in 0..500 {
        source.push_str(&format!(
            "fn function_{i}(arg: i32) -> i32 {{\n    arg + {i}\n}}\n\n"
        ));
    }
    let tokenizer = tokenizer();
    let decision =
        skltn_mcp::budget::should_skeletonize(&source, &tokenizer, CacheHint::CacheConfirmed);
    match decision {
        skltn_mcp::budget::BudgetDecision::ReturnFull { original_tokens } => {
            assert!(original_tokens > 2000);
        }
        _ => panic!("Expected ReturnFull for large file with CacheConfirmed hint"),
    }
}

#[test]
fn test_cache_expired_hint_large_file_returns_skeletonize() {
    let mut source = String::new();
    for i in 0..500 {
        source.push_str(&format!(
            "fn function_{i}(arg: i32) -> i32 {{\n    arg + {i}\n}}\n\n"
        ));
    }
    let tokenizer = tokenizer();
    let decision =
        skltn_mcp::budget::should_skeletonize(&source, &tokenizer, CacheHint::CacheExpired);
    match decision {
        skltn_mcp::budget::BudgetDecision::Skeletonize { original_tokens } => {
            assert!(original_tokens > 2000);
        }
        _ => panic!("Expected Skeletonize for large file with CacheExpired hint"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p skltn-mcp --test budget_test -- --nocapture`
Expected: Compilation error — `CacheHint` doesn't exist, `should_skeletonize` signature mismatch.

- [ ] **Step 3: Update the existing tests to use CacheHint::Unknown**

Replace the existing tests in `crates/skltn-mcp/tests/budget_test.rs` so they pass `CacheHint::Unknown` (preserving backward-compatible behavior):

```rust
use tiktoken_rs::CoreBPE;
use skltn_mcp::budget::CacheHint;

fn tokenizer() -> CoreBPE {
    tiktoken_rs::cl100k_base().unwrap()
}

#[test]
fn test_small_file_returns_full() {
    let source = "fn main() {\n    println!(\"hello\");\n}\n";
    let tokenizer = tokenizer();
    let decision = skltn_mcp::budget::should_skeletonize(source, &tokenizer, CacheHint::Unknown);
    match decision {
        skltn_mcp::budget::BudgetDecision::ReturnFull { original_tokens } => {
            assert!(original_tokens <= 2000);
            assert!(original_tokens > 0);
        }
        _ => panic!("Expected ReturnFull for small file"),
    }
}

#[test]
fn test_large_file_returns_skeletonize() {
    let mut source = String::new();
    for i in 0..500 {
        source.push_str(&format!(
            "fn function_{i}(arg: i32) -> i32 {{\n    arg + {i}\n}}\n\n"
        ));
    }
    let tokenizer = tokenizer();
    let decision = skltn_mcp::budget::should_skeletonize(&source, &tokenizer, CacheHint::Unknown);
    match decision {
        skltn_mcp::budget::BudgetDecision::Skeletonize { original_tokens } => {
            assert!(original_tokens > 2000);
        }
        _ => panic!("Expected Skeletonize for large file"),
    }
}

#[test]
fn test_count_tokens_returns_correct_count() {
    let tokenizer = tokenizer();
    let count = skltn_mcp::budget::count_tokens("hello world", &tokenizer);
    assert!(count > 0);
}
```

- [ ] **Step 4: Implement CacheHint and updated should_skeletonize**

Replace `crates/skltn-mcp/src/budget.rs` with:

```rust
use tiktoken_rs::CoreBPE;

pub const TOKEN_THRESHOLD: usize = 2_000;

/// Hint about whether a file's content is likely in the provider's prompt cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheHint {
    /// No prior information — cold start, use token threshold heuristic.
    Unknown,
    /// File was served full recently in this session — likely cached by provider.
    RecentlyServed,
    /// Phase 3 integration: obs proxy confirmed cache_read_input_tokens > 0.
    CacheConfirmed,
    /// Phase 3 integration: obs data is stale (>5min since last cache hit).
    CacheExpired,
}

#[derive(Debug)]
pub enum BudgetDecision {
    Skeletonize { original_tokens: usize },
    ReturnFull { original_tokens: usize },
}

pub fn should_skeletonize(source: &str, tokenizer: &CoreBPE, hint: CacheHint) -> BudgetDecision {
    let token_count = tokenizer.encode_ordinary(source).len();

    match hint {
        // File is likely cached — serve full regardless of size
        CacheHint::RecentlyServed | CacheHint::CacheConfirmed => BudgetDecision::ReturnFull {
            original_tokens: token_count,
        },
        // No cache info or cache expired — fall back to token threshold
        CacheHint::Unknown | CacheHint::CacheExpired => {
            if token_count > TOKEN_THRESHOLD {
                BudgetDecision::Skeletonize {
                    original_tokens: token_count,
                }
            } else {
                BudgetDecision::ReturnFull {
                    original_tokens: token_count,
                }
            }
        }
    }
}

pub fn count_tokens(text: &str, tokenizer: &CoreBPE) -> usize {
    tokenizer.encode_ordinary(text).len()
}
```

- [ ] **Step 5: Run budget tests to verify they pass**

Run: `cargo test -p skltn-mcp --test budget_test -- --nocapture`
Expected: All 8 tests pass (3 updated originals + 5 new CacheHint tests).

- [ ] **Step 6: Commit**

```bash
git add crates/skltn-mcp/src/budget.rs crates/skltn-mcp/tests/budget_test.rs
git commit -m "feat(mcp): add CacheHint enum and cache-aware should_skeletonize"
```

---

## Chunk 2: SessionTracker

### Task 2: Create SessionTracker module

**Files:**
- Create: `crates/skltn-mcp/src/session.rs`
- Modify: `crates/skltn-mcp/src/lib.rs`
- Create: `crates/skltn-mcp/tests/session_test.rs`

- [ ] **Step 1: Write failing tests for SessionTracker**

Create `crates/skltn-mcp/tests/session_test.rs`:

```rust
use std::path::PathBuf;

use skltn_mcp::budget::CacheHint;
use skltn_mcp::session::SessionTracker;

#[test]
fn test_unknown_hint_for_unseen_file() {
    let tracker = SessionTracker::new();
    let hint = tracker.hint_for(&PathBuf::from("/repo/src/main.rs"));
    assert_eq!(hint, CacheHint::Unknown);
}

#[test]
fn test_recently_served_after_record() {
    let mut tracker = SessionTracker::new();
    let path = PathBuf::from("/repo/src/main.rs");
    tracker.record_full(&path);
    let hint = tracker.hint_for(&path);
    assert_eq!(hint, CacheHint::RecentlyServed);
}

#[test]
fn test_different_file_still_unknown() {
    let mut tracker = SessionTracker::new();
    tracker.record_full(&PathBuf::from("/repo/src/main.rs"));
    let hint = tracker.hint_for(&PathBuf::from("/repo/src/lib.rs"));
    assert_eq!(hint, CacheHint::Unknown);
}

#[test]
fn test_multiple_records_same_file() {
    let mut tracker = SessionTracker::new();
    let path = PathBuf::from("/repo/src/main.rs");
    tracker.record_full(&path);
    tracker.record_full(&path);
    let hint = tracker.hint_for(&path);
    assert_eq!(hint, CacheHint::RecentlyServed);
}

#[test]
fn test_multiple_different_files() {
    let mut tracker = SessionTracker::new();
    let path_a = PathBuf::from("/repo/src/a.rs");
    let path_b = PathBuf::from("/repo/src/b.rs");
    tracker.record_full(&path_a);
    tracker.record_full(&path_b);
    assert_eq!(tracker.hint_for(&path_a), CacheHint::RecentlyServed);
    assert_eq!(tracker.hint_for(&path_b), CacheHint::RecentlyServed);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p skltn-mcp --test session_test -- --nocapture`
Expected: Compilation error — `session` module doesn't exist.

- [ ] **Step 3: Create session.rs and register module**

Create `crates/skltn-mcp/src/session.rs`:

```rust
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::budget::CacheHint;

/// Tracks which files have been served full in the current MCP session.
/// Used to produce `CacheHint::RecentlyServed` for files likely in the
/// provider's prompt cache.
///
/// The tracker's lifetime matches the MCP server process — no eviction needed.
pub struct SessionTracker {
    served_full: HashMap<PathBuf, Instant>,
}

impl SessionTracker {
    pub fn new() -> Self {
        Self {
            served_full: HashMap::new(),
        }
    }

    /// Record that a file was served full (not skeletonized).
    pub fn record_full(&mut self, path: &Path) {
        self.served_full.insert(path.to_path_buf(), Instant::now());
    }

    /// Get a cache hint for a file based on session history.
    pub fn hint_for(&self, path: &Path) -> CacheHint {
        if self.served_full.contains_key(path) {
            CacheHint::RecentlyServed
        } else {
            CacheHint::Unknown
        }
    }
}
```

Add to `crates/skltn-mcp/src/lib.rs`:

```rust
pub mod budget;
pub mod error;
pub mod resolve;
pub mod session;
pub mod tools;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p skltn-mcp --test session_test -- --nocapture`
Expected: All 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/skltn-mcp/src/session.rs crates/skltn-mcp/src/lib.rs crates/skltn-mcp/tests/session_test.rs
git commit -m "feat(mcp): add SessionTracker for cache-aware file serving"
```

---

## Chunk 3: Wire SessionTracker into SkltnServer and read_skeleton

### Task 3: Update SkltnServer and read_skeleton together

**Files:**
- Modify: `crates/skltn-mcp/src/tools/read_skeleton.rs`
- Modify: `crates/skltn-mcp/src/tools/mod.rs` (tool handler)
- Modify: `crates/skltn-mcp/tests/read_skeleton_test.rs`

- [ ] **Step 1: Write failing test for cache-aware read_skeleton**

Add to `crates/skltn-mcp/tests/read_skeleton_test.rs`:

```rust
use std::sync::{Arc, Mutex};
use skltn_mcp::session::SessionTracker;

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
    let tracker = Arc::new(Mutex::new(SessionTracker::new()));

    // First read: should skeletonize (no cache hint)
    let output1 =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "big.rs", &tok, &tracker);
    assert!(output1.contains("skeleton:"), "First read should skeletonize");

    // Second read: should ALSO skeletonize — skeletonized files are NOT
    // recorded in the tracker, so there's no RecentlyServed hint
    let output2 =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "big.rs", &tok, &tracker);
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
    let tracker = Arc::new(Mutex::new(SessionTracker::new()));

    // First read: small file returned full (under threshold), recorded in tracker
    let output1 =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "growing.rs", &tok, &tracker);
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
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "growing.rs", &tok, &tracker);
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
    let tracker = Arc::new(Mutex::new(SessionTracker::new()));

    // First read: small file returned full (under threshold)
    let output1 =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "small.rs", &tok, &tracker);
    assert!(output1.contains("full file"));
    assert!(!output1.contains("cache-aware"), "First read should not be cache-aware");

    // Second read: still under threshold, hint is RecentlyServed but the
    // cache-aware tag only appears when the hint CHANGED the decision
    // (i.e., when the file would have been skeletonized without the hint).
    // Since this file is still small, it would be full anyway — no tag.
    let output2 =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "small.rs", &tok, &tracker);
    assert!(output2.contains("full file"));
    assert!(!output2.contains("cache-aware"), "Small file should never get cache-aware tag");
}
```

- [ ] **Step 2: Write failing test that read_full_symbol does NOT update tracker**

Add to `crates/skltn-mcp/tests/read_skeleton_test.rs`:

```rust
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
    let tracker = Arc::new(Mutex::new(SessionTracker::new()));

    // Call read_full_symbol — this should NOT update the tracker
    let _symbol_output =
        skltn_mcp::tools::read_full_symbol::read_full_symbol(root, "big.rs", "target_symbol", None, &tok);

    // Now call read_skeleton — should still skeletonize (tracker not updated by read_full_symbol)
    let skeleton_output =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "big.rs", &tok, &tracker);
    assert!(
        skeleton_output.contains("skeleton:"),
        "read_full_symbol should not cause read_skeleton to serve full"
    );
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p skltn-mcp --test read_skeleton_test -- --nocapture`
Expected: Compilation error — `read_skeleton_or_full` doesn't accept `tracker` parameter.

- [ ] **Step 4: Add SessionTracker to SkltnServer and update read_skeleton_or_full**

First, in `crates/skltn-mcp/src/tools/mod.rs`, make these changes:

1. Add import at the top (after existing imports):

```rust
use std::sync::Mutex;
use crate::session::SessionTracker;
```

2. Add field to `SkltnServer`:

```rust
#[derive(Clone)]
pub struct SkltnServer {
    root: PathBuf,
    tokenizer: Arc<CoreBPE>,
    session_tracker: Arc<Mutex<SessionTracker>>,
    tool_router: ToolRouter<Self>,
}
```

3. Update `SkltnServer::new()`:

```rust
impl SkltnServer {
    pub fn new(root: PathBuf, tokenizer: CoreBPE) -> Self {
        let tool_router = Self::tool_router();
        Self {
            root,
            tokenizer: Arc::new(tokenizer),
            session_tracker: Arc::new(Mutex::new(SessionTracker::new())),
            tool_router,
        }
    }
}
```

Then replace `crates/skltn-mcp/src/tools/read_skeleton.rs` with:

- [ ] **Step 5: Replace read_skeleton_or_full implementation**

Replace `crates/skltn-mcp/src/tools/read_skeleton.rs` with:

```rust
use std::path::Path;
use std::sync::{Arc, Mutex};

use tiktoken_rs::CoreBPE;

use skltn_core::engine::SkeletonEngine;
use skltn_core::options::SkeletonOptions;

use crate::budget::{self, BudgetDecision};
use crate::error::McpError;
use crate::resolve::resolve_safe_path;
use crate::session::SessionTracker;

use super::{backend_for_extension, has_parse_errors, language_name};

/// Maximum file size we will attempt to read (10 MB).
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Read a source file, returning either its full content (if small or
/// previously served full) or a skeletonized summary (if large and first read).
///
/// The `tracker` records files served full so subsequent reads can skip
/// skeletonization (prompt cache economics — see spec amendment 2026-03-17).
///
/// The returned string includes a metadata header line followed by the content.
/// Error cases return a human-readable error message string.
pub fn read_skeleton_or_full(
    root: &Path,
    file: &str,
    tokenizer: &CoreBPE,
    tracker: &Arc<Mutex<SessionTracker>>,
) -> String {
    // Resolve and validate path
    let path = match resolve_safe_path(root, file) {
        Ok(p) => p,
        Err(e) => return e.to_string(),
    };

    // Check it is a file
    if !path.is_file() {
        return McpError::FileNotFound(file.to_string()).to_string();
    }

    // Guard against very large files
    if let Ok(metadata) = std::fs::metadata(&path) {
        if metadata.len() > MAX_FILE_SIZE {
            return format!(
                "File too large: {} ({} bytes, limit is 10 MB)",
                file,
                metadata.len()
            );
        }
    }

    // Detect language via extension
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let backend = match backend_for_extension(ext) {
        Some(b) => b,
        None => return McpError::UnsupportedLanguage(file.to_string()).to_string(),
    };

    // Read file contents
    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return McpError::FileNotFound(file.to_string()).to_string(),
    };

    let lang = language_name(ext);

    // Check for parse errors once (used in both branches)
    let warning = if has_parse_errors(&source, backend.as_ref()) {
        " | warning: parse errors detected"
    } else {
        ""
    };

    // Get cache hint from session tracker
    let hint = tracker.lock().unwrap().hint_for(&path);

    match budget::should_skeletonize(&source, tokenizer, hint) {
        BudgetDecision::ReturnFull { original_tokens } => {
            // Tag as cache-aware only when the hint actually changed the
            // decision — i.e., the file is above the token threshold and would
            // have been skeletonized without the hint. Small files under the
            // threshold are always served full regardless of hint.
            let cache_aware = original_tokens > budget::TOKEN_THRESHOLD
                && matches!(
                    hint,
                    crate::budget::CacheHint::RecentlyServed
                        | crate::budget::CacheHint::CacheConfirmed
                );

            // Record this file as served full for future cache hints.
            // We record even small files — they are genuinely in the provider's
            // cache after being served, and if the file grows between reads
            // (e.g., user adds code), the hint will correctly prevent
            // skeletonization of the now-larger file.
            tracker.lock().unwrap().record_full(&path);

            let cache_tag = if cache_aware { " (cache-aware)" } else { "" };

            format!(
                "[file: {file} | language: {lang} | tokens: {original_tokens} | full file{cache_tag}{warning}]\n\n{source}"
            )
        }
        BudgetDecision::Skeletonize { original_tokens } => {
            // Skeletonized files are NOT recorded in the tracker.
            // The skeleton token sequence differs from the full file, so it
            // wouldn't benefit from the provider's prompt cache of the full file.
            let opts = SkeletonOptions::default();
            let skeleton = match SkeletonEngine::skeletonize(&source, backend.as_ref(), &opts) {
                Ok(s) => s,
                Err(e) => return format!("Engine error: {e}"),
            };

            let skeleton_tokens = budget::count_tokens(&skeleton, tokenizer);
            let compression = if original_tokens > 0 {
                ((1.0 - skeleton_tokens as f64 / original_tokens as f64) * 100.0) as u32
            } else {
                0
            };

            format!(
                "[file: {file} | language: {lang} | original: {original_tokens} tokens | skeleton: {skeleton_tokens} tokens | compression: {compression}%{warning}]\n\n{skeleton}"
            )
        }
    }
}
```

- [ ] **Step 6: Update existing read_skeleton tests to pass tracker**

Update the existing tests in `crates/skltn-mcp/tests/read_skeleton_test.rs`. The `test_small_file_returned_full`, `test_large_file_returned_skeletonized`, `test_file_not_found`, `test_unsupported_language`, and `test_path_traversal_blocked` tests all need to pass a tracker argument. Add at the top of the file:

```rust
use std::sync::{Arc, Mutex};
use skltn_mcp::session::SessionTracker;
```

Add a helper:

```rust
fn new_tracker() -> Arc<Mutex<SessionTracker>> {
    Arc::new(Mutex::new(SessionTracker::new()))
}
```

Update each existing test to pass `&new_tracker()` as the fourth argument to `read_skeleton_or_full`. For example:

```rust
#[test]
fn test_small_file_returned_full() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let source = "fn main() {\n    println!(\"hello\");\n}\n";
    fs::write(root.join("main.rs"), source).unwrap();

    let tok = tokenizer();
    let output =
        skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "main.rs", &tok, &new_tracker());

    assert!(output.contains("[file: main.rs"));
    assert!(output.contains("full file"));
    assert!(output.contains("fn main()"));
}
```

Apply the same pattern to `test_large_file_returned_skeletonized`, `test_file_not_found`, `test_unsupported_language`, and `test_path_traversal_blocked`.

- [ ] **Step 7: Update the read_skeleton tool handler in mod.rs**

In `crates/skltn-mcp/src/tools/mod.rs`, update the `read_skeleton` method to pass the session tracker:

```rust
    async fn read_skeleton(
        &self,
        Parameters(params): Parameters<ReadSkeletonParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let root = self.root.clone();
        let file = params.file;
        let tokenizer = Arc::clone(&self.tokenizer);
        let tracker = Arc::clone(&self.session_tracker);

        let output = tokio::task::spawn_blocking(move || {
            read_skeleton::read_skeleton_or_full(&root, &file, &tokenizer, &tracker)
        })
        .await
        .map_err(|e| ErrorData::internal_error(format!("Internal error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
```

- [ ] **Step 8: Run all skltn-mcp tests**

Run: `cargo test -p skltn-mcp -- --nocapture`
Expected: All tests pass — existing tests behave identically (Unknown hint = old behavior), new tests verify cache-aware behavior.

- [ ] **Step 9: Commit**

```bash
git add crates/skltn-mcp/src/tools/mod.rs crates/skltn-mcp/src/tools/read_skeleton.rs crates/skltn-mcp/tests/read_skeleton_test.rs
git commit -m "feat(mcp): wire SessionTracker into read_skeleton for cache-aware serving"
```

---

## Chunk 4: Full Workspace Validation

### Task 4: Run full workspace build, test, and lint

**Files:** None (validation only)

- [ ] **Step 1: Run full workspace build**

Run: `cargo build --workspace`
Expected: Clean build, no errors.

- [ ] **Step 2: Run full workspace tests**

Run: `cargo test --workspace`
Expected: All tests pass (skltn-core: 41, skltn-mcp: 53+, total 94+).

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --all-targets --all-features`
Expected: No warnings.

- [ ] **Step 4: Commit any clippy fixes if needed**

If clippy suggests changes:

```bash
git add -A
git commit -m "fix(mcp): address clippy warnings from cache-aware changes"
```

### Task 5: Update PROGRESS.md

**Files:**
- Modify: `PROGRESS.md`

- [ ] **Step 1: Update PROGRESS.md**

Update the Phase 2 amendment section status from "Designed — spec updated, implementation pending" to "Complete — implemented and tested". Update the session log with the implementation session entry.

- [ ] **Step 2: Commit**

```bash
git add PROGRESS.md
git commit -m "chore: update PROGRESS.md — cache-aware Budget Guard implementation complete"
```
