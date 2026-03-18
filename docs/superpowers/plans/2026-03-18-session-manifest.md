# Session Manifest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable cross-session context restoration by recording which files the LLM reads per session and providing a `restore_session` tool that returns a summary (with change detection) or batch-loads previous session skeletons in a single round trip.

**Architecture:** Manifest persistence lives in `SkeletonCache` (sibling `manifest.json` file in `~/.skltn/cache/<project-hash>/`). On session start, existing manifest is rotated to `manifest.previous.json`. A new `restore_session` MCP tool reads the previous manifest, detects changes via content hashes, and returns either a TOC or full content. `read_skeleton_or_full` gains a `record: bool` parameter to control manifest side-effects.

**Tech Stack:** Rust, serde, time, sha2, tokio, rmcp, tempfile (tests)

**Note on test isolation:** `SkeletonCache::new` hashes the project root to create a cache directory under `~/.skltn/cache/`. In tests using `tempdir()`, this means cache files land in `~/.skltn/cache/<hash-of-tempdir>/`, not inside the tempdir itself. This is acceptable — each tempdir path is unique so tests don't collide, and the small files left behind are harmless. A test-only constructor (`new_with_cache_dir`) could be added later but is not needed for correctness.

---

### Task 1: Add `SessionManifest` struct and persistence methods to `SkeletonCache`

**Files:**
- Modify: `crates/skltn-mcp/src/cache.rs`
- Test: `crates/skltn-mcp/tests/manifest_test.rs`

- [ ] **Step 1: Write failing tests for manifest struct and persistence**

Create `crates/skltn-mcp/tests/manifest_test.rs`:

```rust
use std::fs;
use tempfile::tempdir;
use skltn_mcp::cache::SkeletonCache;

#[test]
fn test_record_manifest_entry_stores_file() {
    let dir = tempdir().unwrap();
    let cache = SkeletonCache::new(dir.path()).unwrap();

    cache.record_manifest_entry("src/main.rs");

    let manifest = cache.load_current_manifest().unwrap();
    assert_eq!(manifest.files, vec!["src/main.rs"]);
}

#[test]
fn test_manifest_deduplicates_entries() {
    let dir = tempdir().unwrap();
    let cache = SkeletonCache::new(dir.path()).unwrap();

    cache.record_manifest_entry("src/main.rs");
    cache.record_manifest_entry("src/lib.rs");
    cache.record_manifest_entry("src/main.rs");

    let manifest = cache.load_current_manifest().unwrap();
    assert_eq!(manifest.files, vec!["src/main.rs", "src/lib.rs"]);
}

#[test]
fn test_manifest_preserves_insertion_order() {
    let dir = tempdir().unwrap();
    let cache = SkeletonCache::new(dir.path()).unwrap();

    cache.record_manifest_entry("src/c.rs");
    cache.record_manifest_entry("src/a.rs");
    cache.record_manifest_entry("src/b.rs");

    let manifest = cache.load_current_manifest().unwrap();
    assert_eq!(manifest.files, vec!["src/c.rs", "src/a.rs", "src/b.rs"]);
}

#[test]
fn test_load_previous_manifest_returns_none_on_first_session() {
    let dir = tempdir().unwrap();
    let cache = SkeletonCache::new(dir.path()).unwrap();

    assert!(cache.load_previous_manifest().is_none());
}

#[test]
fn test_manifest_rotation_on_first_write() {
    let dir = tempdir().unwrap();

    // Session 1: write some entries
    {
        let cache = SkeletonCache::new(dir.path()).unwrap();
        cache.record_manifest_entry("src/auth.rs");
        cache.record_manifest_entry("src/api.rs");
        cache.force_flush_manifest();
    }

    // Session 2: first write rotates session 1's manifest
    {
        let cache = SkeletonCache::new(dir.path()).unwrap();
        cache.record_manifest_entry("src/new_file.rs");
        cache.force_flush_manifest();

        // Previous manifest has session 1's files
        let prev = cache.load_previous_manifest().unwrap();
        assert_eq!(prev.files, vec!["src/auth.rs", "src/api.rs"]);

        // Current manifest has session 2's files
        let current = cache.load_current_manifest().unwrap();
        assert_eq!(current.files, vec!["src/new_file.rs"]);
    }
}

#[test]
fn test_manifest_flush_is_atomic() {
    let dir = tempdir().unwrap();
    let cache = SkeletonCache::new(dir.path()).unwrap();

    cache.record_manifest_entry("src/main.rs");
    cache.force_flush_manifest();

    // Verify the file exists and is valid JSON
    let manifest_path = cache.manifest_path();
    let content = fs::read_to_string(&manifest_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["version"], 1);
    assert!(parsed["timestamp"].is_string());
    assert_eq!(parsed["files"][0], "src/main.rs");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p skltn-mcp --test manifest_test 2>&1 | head -30`
Expected: compilation errors — `record_manifest_entry`, `load_current_manifest`, `load_previous_manifest`, `force_flush_manifest`, `manifest_path` do not exist.

- [ ] **Step 3: Add `SessionManifest` struct and manifest fields to `SkeletonCache`**

In `crates/skltn-mcp/src/cache.rs`, add:

```rust
use std::sync::Mutex;
use std::time::Instant;
use time::OffsetDateTime;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionManifest {
    pub version: u32,
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub files: Vec<String>,
}
```

Add fields to `SkeletonCache`:

```rust
pub struct SkeletonCache {
    cache_dir: PathBuf,
    project_root: PathBuf,
    manifest_entries: Mutex<Vec<String>>,
    last_flush: Mutex<Instant>,
    manifest_dirty: Mutex<bool>,
    has_rotated: Mutex<bool>,
}
```

Update `SkeletonCache::new` to initialize the new fields:

```rust
let cache = Self {
    cache_dir,
    project_root: project_root.to_path_buf(),
    manifest_entries: Mutex::new(Vec::new()),
    last_flush: Mutex::new(Instant::now()),
    manifest_dirty: Mutex::new(false),
    has_rotated: Mutex::new(false),
};
```

- [ ] **Step 4: Implement manifest methods**

Add to `impl SkeletonCache`:

```rust
/// Record a file as read in the current session's manifest.
/// Deduplicates and preserves insertion order. Flushes to disk
/// if >5 seconds since last flush.
pub fn record_manifest_entry(&self, file: &str) {
    let mut entries = self.manifest_entries.lock().unwrap();
    if !entries.iter().any(|f| f == file) {
        entries.push(file.to_string());
    }
    *self.manifest_dirty.lock().unwrap() = true;

    let should_flush = self.last_flush.lock().unwrap().elapsed().as_secs() >= 5;
    if should_flush {
        self.flush_manifest_inner(&entries);
    }
}

/// Force-flush the manifest to disk (used in tests and shutdown).
pub fn force_flush_manifest(&self) {
    let entries = self.manifest_entries.lock().unwrap();
    if !entries.is_empty() {
        self.flush_manifest_inner(&entries);
    }
}

/// Load the current session's manifest from disk.
pub fn load_current_manifest(&self) -> Option<SessionManifest> {
    self.load_manifest_file(&self.manifest_path())
}

/// Load the previous session's manifest from disk.
pub fn load_previous_manifest(&self) -> Option<SessionManifest> {
    self.load_manifest_file(&self.previous_manifest_path())
}

/// Path to the current manifest file.
pub fn manifest_path(&self) -> PathBuf {
    self.cache_dir.join("manifest.json")
}

fn previous_manifest_path(&self) -> PathBuf {
    self.cache_dir.join("manifest.previous.json")
}

fn flush_manifest_inner(&self, entries: &[String]) {
    // Rotate on first flush of session
    let mut rotated = self.has_rotated.lock().unwrap();
    if !*rotated {
        let current = self.manifest_path();
        if current.is_file() {
            let _ = fs::rename(&current, self.previous_manifest_path());
        }
        *rotated = true;
    }

    let manifest = SessionManifest {
        version: 1,
        timestamp: OffsetDateTime::now_utc(),
        files: entries.to_vec(),
    };

    let json = match serde_json::to_string_pretty(&manifest) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("Failed to serialize manifest: {e}");
            return;
        }
    };

    // Atomic write: tmp file then rename
    let tmp_path = self.cache_dir.join("manifest.tmp");
    if let Err(e) = fs::write(&tmp_path, json.as_bytes()) {
        tracing::error!("Failed to write manifest tmp: {e}");
        return;
    }
    if let Err(e) = fs::rename(&tmp_path, self.manifest_path()) {
        tracing::error!("Failed to rename manifest tmp: {e}");
        return;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(self.manifest_path(), fs::Permissions::from_mode(0o600));
    }

    *self.last_flush.lock().unwrap() = Instant::now();
    *self.manifest_dirty.lock().unwrap() = false;
}

fn load_manifest_file(&self, path: &Path) -> Option<SessionManifest> {
    let data = fs::read_to_string(path).ok()?;
    match serde_json::from_str(&data) {
        Ok(m) => Some(m),
        Err(e) => {
            tracing::warn!("Corrupted manifest at {}: {e}", path.display());
            None
        }
    }
}
```

Also add a `Drop` impl so the manifest is flushed when the server shuts down:

```rust
impl Drop for SkeletonCache {
    fn drop(&mut self) {
        let dirty = *self.manifest_dirty.lock().unwrap();
        if dirty {
            let entries = self.manifest_entries.lock().unwrap();
            if !entries.is_empty() {
                self.flush_manifest_inner(&entries);
            }
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p skltn-mcp --test manifest_test -v`
Expected: all 6 tests PASS.

- [ ] **Step 6: Run full test suite to check for regressions**

Run: `cargo test -p skltn-mcp`
Expected: all existing tests PASS (no signature changes yet).

- [ ] **Step 7: Commit**

```bash
git add crates/skltn-mcp/src/cache.rs crates/skltn-mcp/tests/manifest_test.rs
git commit -m "feat: add SessionManifest struct and persistence to SkeletonCache"
```

---

### Task 2: Add `record` parameter to `read_skeleton_or_full` and wire manifest recording

**Files:**
- Modify: `crates/skltn-mcp/src/tools/read_skeleton.rs:30` (add `record` param)
- Modify: `crates/skltn-mcp/src/tools/mod.rs:204` (pass `record: true` from tool handler)
- Modify: `crates/skltn-mcp/tests/read_skeleton_test.rs` (update all call sites)
- Test: `crates/skltn-mcp/tests/manifest_test.rs` (add integration test)

- [ ] **Step 1: Write failing test for manifest recording via `read_skeleton_or_full`**

Add to `crates/skltn-mcp/tests/manifest_test.rs`:

```rust
use std::sync::{Arc, Mutex};
use skltn_mcp::session::SessionTracker;

fn tokenizer() -> tiktoken_rs::CoreBPE {
    tiktoken_rs::cl100k_base().unwrap()
}

fn new_tracker() -> Arc<Mutex<SessionTracker>> {
    Arc::new(Mutex::new(SessionTracker::new()))
}

#[test]
fn test_read_skeleton_records_to_manifest() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();

    let cache = SkeletonCache::new(root).unwrap();
    let tok = tokenizer();

    skltn_mcp::tools::read_skeleton::read_skeleton_or_full(
        root, "main.rs", &tok, &new_tracker(), &None, Some(&cache), true,
    );
    cache.force_flush_manifest();

    let manifest = cache.load_current_manifest().unwrap();
    assert_eq!(manifest.files, vec!["main.rs"]);
}

#[test]
fn test_read_skeleton_record_false_does_not_write_manifest() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();

    let cache = SkeletonCache::new(root).unwrap();
    let tok = tokenizer();

    skltn_mcp::tools::read_skeleton::read_skeleton_or_full(
        root, "main.rs", &tok, &new_tracker(), &None, Some(&cache), false,
    );
    cache.force_flush_manifest();

    assert!(cache.load_current_manifest().is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p skltn-mcp --test manifest_test test_read_skeleton_records 2>&1 | head -20`
Expected: compilation error — `read_skeleton_or_full` doesn't accept 7 args.

- [ ] **Step 3: Add `record` parameter to `read_skeleton_or_full`**

In `crates/skltn-mcp/src/tools/read_skeleton.rs`, change the function signature at line 30:

```rust
pub fn read_skeleton_or_full(
    root: &Path,
    file: &str,
    tokenizer: &CoreBPE,
    tracker: &Arc<Mutex<SessionTracker>>,
    savings_writer: &Option<SavingsWriter>,
    skeleton_cache: Option<&SkeletonCache>,
    record: bool,
) -> String {
```

Add manifest recording in both the `ReturnFull` and `Skeletonize` arms. Place the block **before** the final `format!()` return expression (after the savings record write, before the `format!` that returns the string):

```rust
if record {
    if let Some(cache) = skeleton_cache {
        cache.record_manifest_entry(file);
    }
}
```

- [ ] **Step 4: Update the tool handler call site**

In `crates/skltn-mcp/src/tools/mod.rs` at line 205, add `true` for the `record` parameter:

```rust
read_skeleton::read_skeleton_or_full(&root, &file, &tokenizer, &tracker, &savings_writer, skeleton_cache.as_ref().as_ref(), true)
```

- [ ] **Step 5: Update all existing call sites**

In `crates/skltn-mcp/tests/read_skeleton_test.rs`, update every call to `read_skeleton_or_full` to add `true` as the last argument. There are 10 call sites at approximately lines 23, 45, 58, 68, 77, 109, 115, 134, 150, 206.

In `crates/skltn-mcp/examples/smoke_test.rs`, update every call to `read_skeleton_or_full` to add `true` as the last argument. There are approximately 7 call sites.

- [ ] **Step 6: Run all tests**

Run: `cargo test -p skltn-mcp`
Expected: all tests PASS, including the new manifest recording tests.

- [ ] **Step 7: Commit**

```bash
git add crates/skltn-mcp/src/tools/read_skeleton.rs crates/skltn-mcp/src/tools/mod.rs crates/skltn-mcp/tests/read_skeleton_test.rs crates/skltn-mcp/tests/manifest_test.rs crates/skltn-mcp/examples/smoke_test.rs
git commit -m "feat: add record parameter to read_skeleton_or_full for manifest tracking"
```

---

### Task 3: Implement `restore_session` tool

**Files:**
- Create: `crates/skltn-mcp/src/tools/restore_session.rs`
- Modify: `crates/skltn-mcp/src/tools/mod.rs` (add module, params, tool registration)
- Test: `crates/skltn-mcp/tests/restore_session_test.rs`

- [ ] **Step 1: Write failing tests for restore_session TOC mode**

Create `crates/skltn-mcp/tests/restore_session_test.rs`:

```rust
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

    // Modify only main.rs
    fs::write(root.join("main.rs"), "fn main() { println!(\"changed\"); }\n").unwrap();

    let output = skltn_mcp::tools::restore_session::restore_session(
        root, &tokenizer(), &cache, true, true,
    );
    // Should contain main.rs content but not lib.rs content
    assert!(output.contains("fn main()"));
    assert!(!output.contains("pub fn hello()"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p skltn-mcp --test restore_session_test 2>&1 | head -20`
Expected: compilation error — `restore_session` module doesn't exist.

- [ ] **Step 3: Create `restore_session.rs` with the core logic**

Create `crates/skltn-mcp/src/tools/restore_session.rs`.

**Note:** The `analyze_file` function uses `skel_cache` as the parameter name (not `cache`) to avoid shadowing the `crate::cache` module import. All `cache::hash_content` and `cache::mtime_secs` calls use the full `crate::cache::` path for clarity.

```rust
use std::path::Path;

use tiktoken_rs::CoreBPE;

use crate::cache::SkeletonCache;
use crate::resolve::resolve_safe_path;

use super::language_name;

/// Maximum total tokens to return in load mode.
const MAX_RESTORE_TOKENS: usize = 50_000;

/// Staleness tag for a file in the manifest.
#[derive(Debug, Clone, Copy)]
enum FileStatus {
    Unchanged,
    Modified,
    Removed,
}

impl std::fmt::Display for FileStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileStatus::Unchanged => write!(f, "unchanged"),
            FileStatus::Modified => write!(f, "modified since last session"),
            FileStatus::Removed => write!(f, "removed"),
        }
    }
}

struct ManifestEntry {
    file: String,
    status: FileStatus,
    language: String,
    token_estimate: usize,
}

/// Restore context from the previous session.
///
/// - `load=false`: returns a table of contents with change tags.
/// - `load=true`: returns full skeleton content for each file.
/// - `only_changed=true` (with `load=true`): only loads modified files.
pub fn restore_session(
    root: &Path,
    tokenizer: &CoreBPE,
    skel_cache: &SkeletonCache,
    load: bool,
    only_changed: bool,
) -> String {
    let manifest = match skel_cache.load_previous_manifest() {
        Some(m) => m,
        None => return "No previous session found for this project.".to_string(),
    };

    if manifest.files.is_empty() {
        return "No previous session found for this project.".to_string();
    }

    // Analyze each file
    let entries: Vec<ManifestEntry> = manifest
        .files
        .iter()
        .map(|file| analyze_file(root, file, skel_cache))
        .collect();

    if load {
        build_load_response(root, tokenizer, &entries, only_changed, skel_cache)
    } else {
        build_toc_response(&entries, &manifest.timestamp)
    }
}

fn analyze_file(root: &Path, file: &str, skel_cache: &SkeletonCache) -> ManifestEntry {
    let ext = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let lang = language_name(ext).to_string();

    // Check if file still exists
    let resolved = match resolve_safe_path(root, file) {
        Ok(p) if p.is_file() => p,
        _ => {
            return ManifestEntry {
                file: file.to_string(),
                status: FileStatus::Removed,
                language: lang,
                token_estimate: 0,
            };
        }
    };

    // Read current source and check for changes
    let source = match std::fs::read_to_string(&resolved) {
        Ok(s) => s,
        Err(_) => {
            return ManifestEntry {
                file: file.to_string(),
                status: FileStatus::Removed,
                language: lang,
                token_estimate: 0,
            };
        }
    };

    let current_hash = crate::cache::hash_content(&source);
    let mtime = std::fs::metadata(&resolved)
        .ok()
        .map(|m| crate::cache::mtime_secs(&m))
        .unwrap_or(0);

    // Check cache entry for the stored hash
    let cached = skel_cache.get_with_validation(file, mtime, &source);
    let status = match &cached {
        Some(entry) if entry.content_hash == current_hash => FileStatus::Unchanged,
        _ => FileStatus::Modified,
    };

    // Token estimate from cache or raw count
    let token_estimate = match &cached {
        Some(entry) => entry.skeleton_tokens.max(entry.original_tokens),
        None => tokenizer_estimate(&source),
    };

    ManifestEntry {
        file: file.to_string(),
        status,
        language: lang,
        token_estimate,
    }
}

fn tokenizer_estimate(source: &str) -> usize {
    // Rough estimate: 1 token per 4 bytes
    source.len() / 4
}

fn build_toc_response(
    entries: &[ManifestEntry],
    timestamp: &time::OffsetDateTime,
) -> String {
    let total_files = entries.len();
    let modified_count = entries
        .iter()
        .filter(|e| matches!(e.status, FileStatus::Modified))
        .count();
    let removed_count = entries
        .iter()
        .filter(|e| matches!(e.status, FileStatus::Removed))
        .count();
    let total_tokens: usize = entries.iter().map(|e| e.token_estimate).sum();

    let mut out = format!(
        "Previous session: {total_files} files | {timestamp}\n\n"
    );

    for entry in entries {
        let tokens = if entry.token_estimate > 0 {
            format!("{} tokens", entry.token_estimate)
        } else {
            "-".to_string()
        };
        out.push_str(&format!(
            "  {:<40} | {:<12} | {:<14} | {}\n",
            entry.file, entry.language, tokens, entry.status
        ));
    }

    out.push_str(&format!(
        "\nTotal: ~{total_tokens} tokens if fully restored."
    ));

    if modified_count > 0 || removed_count > 0 {
        out.push_str(&format!(
            " ({modified_count} modified, {removed_count} removed)"
        ));
    }

    out.push_str(
        "\nCall read_skeleton on individual files, or restore_session with load=true to load all.",
    );

    out
}

fn build_load_response(
    root: &Path,
    tokenizer: &CoreBPE,
    entries: &[ManifestEntry],
    only_changed: bool,
    skel_cache: &SkeletonCache,
) -> String {
    let mut out = String::new();
    let mut total_tokens: usize = 0;
    let mut loaded: usize = 0;
    let mut omitted: usize = 0;

    let tracker = std::sync::Arc::new(std::sync::Mutex::new(
        crate::session::SessionTracker::new(),
    ));

    for entry in entries {
        if matches!(entry.status, FileStatus::Removed) {
            continue;
        }
        if only_changed && matches!(entry.status, FileStatus::Unchanged) {
            continue;
        }

        // Check budget before loading
        if total_tokens + entry.token_estimate > MAX_RESTORE_TOKENS {
            omitted += 1;
            continue;
        }

        // Use read_skeleton_or_full with record=false
        let content = super::read_skeleton::read_skeleton_or_full(
            root,
            &entry.file,
            tokenizer,
            &tracker,
            &None,
            Some(skel_cache),
            false,
        );

        if !out.is_empty() {
            out.push_str("\n\n---\n\n");
        }

        // Add staleness tag to the header line
        let status_tag = format!(" | {}", entry.status);
        // Insert status tag before the closing ]
        let tagged = if let Some(bracket_pos) = content.find("]\n") {
            let mut tagged = content.clone();
            tagged.insert_str(bracket_pos, &status_tag);
            tagged
        } else {
            content
        };

        total_tokens += entry.token_estimate;
        loaded += 1;
        out.push_str(&tagged);
    }

    if omitted > 0 {
        let omitted_tokens: usize = entries
            .iter()
            .rev()
            .take(omitted)
            .map(|e| e.token_estimate)
            .sum();
        out.push_str(&format!(
            "\n\n---\n\n[truncated: {loaded}/{} files loaded (~{total_tokens} tokens). \
             {omitted} files omitted (estimated ~{omitted_tokens} tokens). \
             Call read_skeleton individually for remaining files.]",
            entries.len()
        ));
    }

    if out.is_empty() {
        "No files to restore (all removed or filtered).".to_string()
    } else {
        out
    }
}
```

- [ ] **Step 4: Add module declaration**

In `crates/skltn-mcp/src/tools/mod.rs`, add at the top with the other module declarations:

```rust
pub mod restore_session;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p skltn-mcp --test restore_session_test -v`
Expected: all 6 tests PASS.

- [ ] **Step 6: Run full test suite**

Run: `cargo test -p skltn-mcp`
Expected: all tests PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/skltn-mcp/src/tools/restore_session.rs crates/skltn-mcp/src/tools/mod.rs crates/skltn-mcp/tests/restore_session_test.rs
git commit -m "feat: implement restore_session core logic with TOC and load modes"
```

---

### Task 4: Register `restore_session` as an MCP tool

**Files:**
- Modify: `crates/skltn-mcp/src/tools/mod.rs` (add params struct, tool handler, update server info)
- Modify: `crates/skltn-mcp/tests/mcp_integration_test.rs` (update tool count, add registration test)

- [ ] **Step 1: Write failing test for tool registration**

Add to `crates/skltn-mcp/tests/mcp_integration_test.rs`:

```rust
#[tokio::test]
async fn test_server_registers_restore_session_tool() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());
    let server = setup_server(dir.path());

    let tool = server
        .get_tool("restore_session")
        .expect("restore_session tool should be registered");
    assert_eq!(tool.name.as_ref(), "restore_session");
    assert!(tool.description.is_some());
}
```

Also update `test_server_has_exactly_three_tools` to expect 4 tools and rename it to `test_server_has_exactly_four_tools`.

Update `test_server_get_info_has_instructions` to also check for `restore_session` in instructions.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p skltn-mcp --test mcp_integration_test test_server_registers_restore_session 2>&1 | head -20`
Expected: FAIL — `restore_session` tool not registered.

- [ ] **Step 3: Add `RestoreSessionParams` struct**

In `crates/skltn-mcp/src/tools/mod.rs`, add with the other param structs:

```rust
fn default_false() -> bool {
    false
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RestoreSessionParams {
    /// Load full file contents. When false (default), returns a summary table
    /// with change annotations. When true, returns skeleton/full content for
    /// each file from the previous session.
    #[serde(default = "default_false")]
    pub load: bool,
    /// Only load files that changed since last session. Only applies when
    /// load=true. Ignored when load=false (summary always shows all files).
    #[serde(default = "default_false")]
    pub only_changed: bool,
}
```

- [ ] **Step 4: Add the tool handler**

In the `#[tool_router] impl SkltnServer` block, add:

```rust
/// Restore context from the previous session. Returns a summary of files
/// the LLM read last session with change annotations. Use load=true to
/// load all file contents in a single batch. Use only_changed=true with
/// load=true to load only files that changed since last session.
#[tool(
    name = "restore_session",
    description = "Restore context from the previous session. Returns a summary of files read last session with change annotations. Use load=true to load contents, only_changed=true to filter to modified files."
)]
async fn restore_session(
    &self,
    Parameters(params): Parameters<RestoreSessionParams>,
) -> Result<CallToolResult, ErrorData> {
    let root = self.root.clone();
    let tokenizer = Arc::clone(&self.tokenizer);
    let skeleton_cache = Arc::clone(&self.skeleton_cache);
    let load = params.load;
    let only_changed = params.only_changed;

    let output = tokio::task::spawn_blocking(move || {
        match skeleton_cache.as_ref() {
            Some(c) => {
                restore_session::restore_session(&root, &tokenizer, c, load, only_changed)
            }
            None => "Session manifest unavailable (cache not initialized).".to_string(),
        }
    })
    .await
    .map_err(|e| ErrorData::internal_error(format!("Internal error: {e}"), None))?;

    Ok(CallToolResult::success(vec![Content::text(output)]))
}
```

- [ ] **Step 5: Update server instructions**

In the `get_info` method, update the instructions string to mention `restore_session`:

```rust
.with_instructions(
    "Skeleton (skltn) MCP server. Navigate codebases efficiently: \
     list_repo_structure -> read_skeleton -> read_full_symbol. \
     Use restore_session to reload context from a previous session."
        .to_string(),
)
```

- [ ] **Step 6: Run all tests**

Run: `cargo test -p skltn-mcp`
Expected: all tests PASS, including the new registration tests.

- [ ] **Step 7: Run clippy**

Run: `cargo clippy -p skltn-mcp --all-targets --all-features`
Expected: no warnings.

- [ ] **Step 8: Commit**

```bash
git add crates/skltn-mcp/src/tools/mod.rs crates/skltn-mcp/tests/mcp_integration_test.rs
git commit -m "feat: register restore_session as MCP tool with load and only_changed params"
```

---

### Task 5: Token budget guard test and edge case hardening

**Files:**
- Modify: `crates/skltn-mcp/tests/restore_session_test.rs` (add budget and edge case tests)
- Modify: `crates/skltn-mcp/src/tools/restore_session.rs` (fix any issues found)

- [ ] **Step 1: Write test for token budget truncation**

Add to `crates/skltn-mcp/tests/restore_session_test.rs`:

```rust
#[test]
fn test_restore_load_respects_token_budget() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Create many large files that together exceed 50k tokens
    for i in 0..30 {
        let mut source = String::new();
        for j in 0..100 {
            source.push_str(&format!(
                "pub fn func_{i}_{j}(x: i32) -> i32 {{\n    let a = x + 1;\n    let b = a * 2;\n    let c = b - 3;\n    c + {j}\n}}\n\n"
            ));
        }
        fs::write(root.join(format!("mod_{i}.rs")), &source).unwrap();
    }

    let files: Vec<(&str, &str)> = (0..30)
        .map(|i| {
            // Leak is fine in tests
            let name: &str = Box::leak(format!("mod_{i}.rs").into_boxed_str());
            (name, "")
        })
        .collect();

    let cache = setup_two_sessions(root, &files);

    let output = skltn_mcp::tools::restore_session::restore_session(
        root, &tokenizer(), &cache, true, false,
    );

    // Should contain truncation notice
    assert!(output.contains("truncated"));
    assert!(output.contains("files omitted"));
}

#[test]
fn test_restore_with_empty_manifest() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Session 1: create cache but read nothing (empty manifest won't be written)
    {
        let _cache = SkeletonCache::new(root).unwrap();
    }

    // Session 2
    let cache = SkeletonCache::new(root).unwrap();

    let output = skltn_mcp::tools::restore_session::restore_session(
        root, &tokenizer(), &cache, false, false,
    );
    assert!(output.contains("No previous session found"));
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p skltn-mcp --test restore_session_test -v`
Expected: all tests PASS.

- [ ] **Step 3: Run full suite + clippy**

Run: `cargo test -p skltn-mcp && cargo clippy -p skltn-mcp --all-targets --all-features`
Expected: all PASS, no warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/skltn-mcp/tests/restore_session_test.rs crates/skltn-mcp/src/tools/restore_session.rs
git commit -m "test: add token budget and edge case tests for restore_session"
```
