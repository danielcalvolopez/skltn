# Cross-Session Skeleton Cache

## Problem

Every skltn session starts cold. The `SessionTracker` is in-memory only and `savings.jsonl` is truncated on proxy restart. If Claude reads 90 files in session 1, session 2 re-parses and re-skeletonizes every one of them from scratch. The AST parsing and tokenization cost is paid repeatedly for files that haven't changed.

## Solution

A per-project, file-based skeleton cache that persists skeletonized output between sessions. On `read_skeleton`, the system checks the cache before doing any AST work. Cache entries are invalidated by content hash, with file mtime as a cheap pre-filter.

## Storage Layout

```
~/.skltn/cache/<project-hash>/
  ├── src__components__UserProfile.tsx.json
  ├── src__lib__auth.rs.json
  └── ...
```

- **Project hash**: SHA-256 of the canonicalized project root path, truncated to 16 hex chars
- **File key**: relative path with `/` replaced by `__`, plus `.json` extension
- One JSON file per cached skeleton

## Cache Entry Schema

```json
{
  "content_hash": "a3f2b1c4d5e6f7a8b9c0d1e2f3a4b5c6a3f2b1c4d5e6f7a8b9c0d1e2f3a4b5c6",
  "mtime_secs": 1742307600,
  "original_tokens": 6764,
  "skeleton_tokens": 1311,
  "has_parse_errors": false,
  "skeleton": "[skltn: ...]\n\npub struct ChatContext {\n  ..."
}
```

| Field | Type | Purpose |
|---|---|---|
| `content_hash` | String | SHA-256 of file contents (64 hex chars). Authoritative invalidation key. |
| `mtime_secs` | i64 | File mtime at time of caching (Unix timestamp). Cheap pre-filter. |
| `original_tokens` | usize | Token count of the original file. Avoids tokenizer on cache hit. |
| `skeleton_tokens` | usize | Token count of the skeleton. Avoids tokenizer on cache hit. |
| `has_parse_errors` | bool | Whether the source had parse errors. Needed for metadata header reconstruction. |
| `skeleton` | String | Full skeleton text (without metadata header — reconstructed on read). |

## Read Path

Modified `read_skeleton_or_full` flow:

```
read_skeleton(file)
  ├─ resolve path, read file metadata
  ├─ check SessionTracker hint (existing logic, unchanged)
  ├─ if BudgetDecision::ReturnFull → return full (unchanged, no cache interaction)
  ├─ if BudgetDecision::Skeletonize:
  │   ├─ stat file → get mtime
  │   ├─ cache.get_with_validation(file, mtime)
  │   │   ├─ no cache entry → CACHE MISS
  │   │   ├─ cache.mtime_secs == mtime → CACHE HIT (fast path)
  │   │   ├─ mtime differs → hash file contents
  │   │   │   ├─ cache.content_hash == hash → CACHE HIT (update mtime in entry)
  │   │   │   └─ hash differs → CACHE MISS
  │   │
  │   ├─ CACHE HIT: return cached skeleton + emit SavingsRecord from cached metadata
  │   └─ CACHE MISS: skeletonize via engine → write cache entry → emit SavingsRecord → return
```

Key design decisions:
- Cache sits **inside** the `Skeletonize` branch only. Files served full bypass the cache entirely.
- The budget decision (`should_skeletonize`) still runs on every call — it requires the source text and tokenizer. The cache skips AST parsing and skeletonization, not the budget check.
- `SavingsRecord` is always emitted regardless of cache hit/miss — dashboard metrics are unaffected.
- The mtime update on content-hash hit prevents re-hashing after git branch switches (git updates mtime but content may be identical).
- On cache hit, `has_parse_errors` from the entry is used to reconstruct the metadata header warning without re-parsing.

## Cache Validation Method

The `get_with_validation` method encapsulates the two-tier validation:

```rust
impl SkeletonCache {
    /// Returns a hit if mtime or content hash matches.
    /// If mtime misses but hash hits, updates the stored mtime to avoid future hashing.
    pub fn get_with_validation(&self, file: &str, current_mtime: i64) -> Option<CacheEntry> {
        let entry = self.load(file)?;

        // Fast path: mtime unchanged
        if entry.mtime_secs == current_mtime {
            return Some(entry);
        }

        // Slow path: mtime changed, check content hash
        let current_hash = self.hash_file(file).ok()?;
        if entry.content_hash == current_hash {
            // Content identical — update mtime to avoid future hashing
            let mut updated = entry;
            updated.mtime_secs = current_mtime;
            self.store(file, &updated);
            return Some(updated);
        }

        None // Real cache miss — content changed
    }
}
```

## Startup Cleanup

On `SkltnServer` initialization, before accepting requests:

1. Resolve project cache directory (`~/.skltn/cache/<project-hash>/`)
2. Scan all `.json` entries in the directory
3. For each entry, derive the source file path from the filename
4. `stat` the source file — if it no longer exists, delete the cache entry
5. Log count of evicted entries at `tracing::info` level

Performance: ~500 stat calls for a large project. Negligible latency.

## New Module: `crates/skltn-mcp/src/cache.rs`

```rust
use std::path::{Path, PathBuf};

pub struct SkeletonCache {
    cache_dir: PathBuf,
    project_root: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CacheEntry {
    pub content_hash: String,
    pub mtime_secs: i64,
    pub original_tokens: usize,
    pub skeleton_tokens: usize,
    pub has_parse_errors: bool,
    pub skeleton: String,
}

impl SkeletonCache {
    /// Create a new cache for the given project root.
    /// Creates the cache directory if needed and runs startup cleanup.
    pub fn new(project_root: &Path) -> Option<Self>;

    /// Two-tier cache lookup: mtime first, then content hash.
    /// Updates stored mtime on content-hash hit.
    pub fn get_with_validation(&self, file: &str, current_mtime: i64) -> Option<CacheEntry>;

    /// Store a skeleton cache entry for a file.
    pub fn store(&self, file: &str, entry: &CacheEntry);

    /// Remove cache entries for files that no longer exist.
    fn cleanup(&self);
}
```

## Changes to Existing Code

| Component | Change | Scope |
|---|---|---|
| `SkltnServer` (lib.rs) | Add `Option<Arc<SkeletonCache>>` field, initialize in constructor, add `pub mod cache;` | Small |
| `read_skeleton_or_full` (read_skeleton.rs) | New `cache: Option<&SkeletonCache>` parameter, lookup/store in `Skeletonize` branch | Medium |
| `cache.rs` | New module — all cache logic | New file |
| `Cargo.toml` (skltn-mcp) | Add `sha2` dependency | One line |

### What does NOT change

- `skltn-core` (engine, backends, options) — untouched
- `budget.rs` — decision logic unchanged
- `session.rs` — SessionTracker unchanged, still handles within-session cache-aware decisions
- `savings.rs` / `SavingsWriter` — still emits records on every read
- `skltn-obs` — dashboard, proxy, WebSocket — no changes needed
- Existing tests require passing the new `cache` parameter (`None`) to `read_skeleton_or_full` — trivial signature update, no behavioral change

## New Dependency

- `sha2` crate for SHA-256 hashing. Widely used, no transitive bloat. ~30KB.

## Edge Cases

| Scenario | Behavior |
|---|---|
| File edited mid-session | mtime changes on save → cache miss on next read → re-skeletonize |
| Git branch switch (same content) | mtime changes → hash check → content match → cache hit, mtime updated |
| Git branch switch (different content) | mtime changes → hash check → content differs → cache miss → re-skeletonize |
| File deleted | Startup cleanup removes entry. Mid-session: `read_skeleton` returns file-not-found before cache is consulted. |
| File renamed | Old entry cleaned up on next startup. New path = new cache entry. |
| Corrupted cache entry JSON | `serde_json::from_str` fails → treated as cache miss, log warning, re-skeletonize |
| Cache dir permissions | Same `0o600` pattern as existing JSONL files |
| Very large project (500+ files) | 500 small JSON files (~2MB total). Startup cleanup: ~500 stat calls. No concern. |
| Project root path changes | New project hash → new cache directory. Old directory is orphaned but harmless. |

## Performance Impact

The budget decision (`should_skeletonize`) still runs on every call because it needs the source text and token count. The cache skips the expensive operations that happen *after* the budget decision: AST parsing, skeletonization, and the second tokenization pass.

| Operation | Without cache | With cache (hit) | With cache (miss) |
|---|---|---|---|
| `read_skeleton` | stat + read + tokenize + AST parse + skeletonize + tokenize | stat + read + tokenize + read JSON | stat + read + tokenize + hash + AST parse + skeletonize + tokenize + write JSON |
| Latency (typical) | ~5-15ms | ~2-3ms | ~6-16ms (one-time) |

Cache hits skip AST parsing and skeletonization — the two most expensive operations. The file read and first tokenization pass (for the budget decision) still occur. Cache misses add hash computation + JSON write overhead.

## Testing Strategy

1. **Unit tests** (`crates/skltn-mcp/src/cache.rs`):
   - Store and retrieve a cache entry
   - mtime-match fast path returns hit
   - mtime-mismatch with same content returns hit and updates mtime
   - mtime-mismatch with different content returns miss
   - Corrupted JSON file treated as miss
   - Cleanup removes entries for deleted files

2. **Integration test** (new file in `crates/skltn-mcp/tests/`):
   - Full round-trip: `read_skeleton_or_full` populates cache, second call returns cached result
   - Modify file → second call re-skeletonizes
