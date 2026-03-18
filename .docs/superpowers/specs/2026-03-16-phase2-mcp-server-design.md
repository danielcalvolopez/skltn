# Phase 2: MCP Server — Design Specification

**Project:** skltn (Skeleton)
**Phase:** 2 of 4
**Date:** 2026-03-16
**Status:** Approved

---

## Overview

The MCP Server is a Rust binary that exposes the Skeleton Engine (Phase 1) over the Model Context Protocol. It provides three tools — `list_repo_structure`, `read_skeleton`, and `read_full_symbol` — enabling AI agents to navigate codebases efficiently. A Budget Guard uses real token counting to automatically decide whether to return full files or skeletons.

This is Phase 2 of the skltn project. It depends on `skltn-core` (Phase 1) and produces a standalone MCP server binary that can be configured in any MCP-compatible client (e.g., Claude Desktop).

---

## Guiding Principles

1. **Three tools, clean progression.** The AI's workflow is: list → skeleton → hydrate. Each tool has one clear purpose.
2. **Stateless server.** No cached state beyond the repo root path. Every tool call reads fresh from disk. The process lifetime is the session.
3. **Content responses over protocol errors.** Operational feedback (file not found, ambiguous symbol) is returned as successful content the AI can reason about. Protocol errors are reserved for fundamentally broken requests.

---

## Architecture

### Crate Structure

```
skltn/
├── Cargo.toml                  # Workspace root (add skltn-mcp to members)
├── crates/
│   ├── skltn-core/             # Library — Skeleton Engine (Phase 1, unchanged)
│   ├── skltn-cli/              # Binary — CLI wrapper (Phase 1, unchanged)
│   └── skltn-mcp/              # Binary — MCP server (Phase 2, NEW)
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs         # Server bootstrap, stdio transport, server struct
│           ├── tools/
│           │   ├── mod.rs      # Tool registration
│           │   ├── list_repo_structure.rs
│           │   ├── read_skeleton.rs
│           │   └── read_full_symbol.rs
│           ├── budget.rs       # Budget Guard (tiktoken-rs token counting)
│           ├── resolve.rs      # Symbol resolution + path security
│           └── error.rs        # MCP error mapping (protocol vs content)
```

### Dependencies

| Crate | Purpose |
|---|---|
| `skltn-core` | Skeletonization engine (workspace dependency) |
| `rmcp` | MCP protocol server, stdio transport |
| `tiktoken-rs` | Token counting for Budget Guard |
| `ignore` | `.gitignore`-aware directory walking |
| `serde`, `serde_json` | MCP message serialization |
| `tokio` | Async runtime (required by `rmcp`) |

### Server Bootstrap

The binary takes a single required argument — the repository root path:

```
skltn-mcp <ROOT_PATH>
```

On startup:
1. Validate the root path exists and is a directory
2. Initialize the `tiktoken-rs` `CoreBPE` tokenizer (`cl100k_base` model) — initialized once, shared across tool calls
3. Initialize the `rmcp` stdio server
4. Register the three tools
5. Block on the transport loop

The server struct holds the root path (`PathBuf`) and the tokenizer instance. No other state. The MCP client (e.g., Claude Desktop) starts and stops the process — the process lifecycle is the session lifecycle.

### Async Considerations

tree-sitter parsing and `tiktoken-rs` token counting are CPU-bound, synchronous operations. To keep the MCP transport loop responsive, these are wrapped in `tokio::task::spawn_blocking` when called from async tool handlers.

---

## Tool 1: `list_repo_structure`

**Purpose:** Give the AI a map of the repository so it can decide which files to inspect.

### Parameters

| Parameter | Type | Required | Default | Description |
|---|---|---|---|---|
| `path` | string | No | `"."` (repo root) | Subdirectory to list, relative to repo root |
| `max_depth` | number | No | `null` (unlimited) | Maximum directory depth to traverse |

### Behavior

1. Resolve `path` relative to repo root (with path security check — see Path Security section)
2. Walk the directory using the `ignore` crate (respects `.gitignore`)
3. Filter to files with supported extensions (`.rs`, `.py`, `.ts`, `.js`)
4. Show directories for structural context. Directories that contain no supported files (recursively) are pruned from the output to avoid confusing empty entries
5. If `max_depth` is provided, limit directory traversal depth accordingly
6. Return a tree-style listing with metadata per file

### Response Format

```
src/
  engine.rs (4,821 bytes, rust)
  lib.rs (342 bytes, rust)
  backend/
    mod.rs (1,205 bytes, rust)
    rust.rs (2,847 bytes, rust)
    python.rs (3,102 bytes, rust)
tests/
  integration.rs (956 bytes, rust)
```

- Indentation indicates nesting depth
- Trailing `/` distinguishes directories from files
- Each file shows byte size and detected language
- Unsupported files are omitted — the AI only sees files it can skeletonize

### Edge Cases

| Condition | Response |
|---|---|
| Path doesn't exist | Content response: `"Directory not found: {path}"` |
| Path is a file, not a directory | Content response: `"Path is a file, not a directory: {path}. Use read_skeleton to inspect it."` |
| Directory exists but contains no supported files | Content response: `"No supported source files (.rs, .py, .ts, .js) found in {path}"` |
| Path traversal attempt | Content response: `"Path is outside the repository root"` |

---

## Tool 2: `read_skeleton`

**Purpose:** Return the skeletonized version of a single file, or the full file if it's small enough.

### Parameters

| Parameter | Type | Required | Default | Description |
|---|---|---|---|---|
| `file` | string | Yes | — | File path relative to repo root |

### Behavior

1. Resolve path relative to repo root (with path security check)
2. If file doesn't exist → content response: `"File not found: {path}"`
3. Check file size. If > 10 MB → content response: `"File too large: {path} ({size} bytes, limit is 10 MB)"`
4. Detect language from extension. If unsupported → content response: `"Unsupported language for file: {path}. Supported: .rs, .py, .ts, .js"`
5. Read file contents
6. Run `tiktoken-rs` token count on the source
7. **Budget Guard decision:**
   - Token count ≤ 2,000 → return the full file contents (no skeletonization needed)
   - Token count > 2,000 → skeletonize via `skltn-core::SkeletonEngine::skeletonize()` and return the skeleton
8. Run `tiktoken-rs` token count on the output (whether full or skeleton)
9. Return with metadata header

### Response Format

**Skeletonized file (>2k tokens):**
```
[file: src/engine.rs | language: rust | original: 4,821 tokens | skeleton: 847 tokens | compression: 82%]

pub struct SkeletonEngine;

impl SkeletonEngine {
    pub fn skeletonize(source: &str, ...) -> Result<String, SkltnError> {
        todo!() // [skltn: 45 lines hidden]
    }
}
```

**Full file (≤2k tokens):**
```
[file: src/options.rs | language: rust | tokens: 342 | full file]

pub struct SkeletonOptions {
    pub max_depth: Option<usize>,
}
```

**File with syntax errors:**
```
[file: src/broken.rs | language: rust | original: 3,100 tokens | skeleton: 650 tokens | compression: 79% | warning: parse errors detected]

// Skeleton output with ERROR nodes emitted verbatim (same as Phase 1 engine behavior)
```

### Budget Guard Details

The Budget Guard lives in `budget.rs`. It is a simple gate function:

- **Tokenizer model:** `cl100k_base` — a widely available BPE tokenizer that provides a reasonable approximation for budget decisions (±5% margin is acceptable for a threshold check)
- **Threshold:** `TOKEN_THRESHOLD = 2_000` — a constant, not configurable via tool parameters or CLI flags
- **Token counts are always real** — both the original file and the output are counted via `tiktoken-rs`, never estimated

```rust
const TOKEN_THRESHOLD: usize = 2_000;

pub enum BudgetDecision {
    Skeletonize { original_tokens: usize },
    ReturnFull { original_tokens: usize },
}

pub fn should_skeletonize(source: &str, tokenizer: &CoreBPE) -> BudgetDecision {
    let token_count = tokenizer.encode_ordinary(source).len();
    if token_count > TOKEN_THRESHOLD {
        BudgetDecision::Skeletonize { original_tokens: token_count }
    } else {
        BudgetDecision::ReturnFull { original_tokens: token_count }
    }
}
```

---

## Tool 3: `read_full_symbol`

**Purpose:** Return the full, unmodified source code of a specific symbol (function, method, struct, class, impl block, etc.).

### Parameters

| Parameter | Type | Required | Default | Description |
|---|---|---|---|---|
| `file` | string | Yes | — | File path relative to repo root |
| `symbol` | string | Yes | — | Symbol name to find (e.g., `"skeletonize"`, `"UserProfile"`) |
| `start_line` | number | No | `null` | Line number hint for disambiguation (1-indexed) |

### Behavior

1. Resolve path and read file (same not-found / unsupported / too-large handling as `read_skeleton`)
2. Parse the file with tree-sitter via `skltn-core`'s language backend
3. Walk the AST using the symbol resolution algorithm (see Symbol Resolution section)
4. For successful matches, run `tiktoken-rs` token count on the extracted source text for the metadata header
5. Handle results:

**Single match → return full source:**
```
[symbol: skeletonize | file: src/engine.rs | lines: 42-89 | 847 tokens]

pub fn skeletonize(
    source: &str,
    backend: &dyn LanguageBackend,
    options: &SkeletonOptions,
) -> Result<String, SkltnError> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(backend.language())?;
    // ... full implementation
}
```

**Multiple matches, no `start_line` → disambiguation list:**
```
Multiple matches for 'new':
  - new (lines 42-58) in impl UserProfile
  - new (lines 104-112) in impl SessionManager

Please re-call with start_line to select one.
```

**Multiple matches, `start_line` provided → return closest match:**

The match whose start line is closest to the provided `start_line` is returned.

**No match:**
```
Symbol 'foo' not found in src/engine.rs
```

**Match found but file has syntax errors:**
```
[symbol: run | file: src/main.rs | lines: 10-25 | 312 tokens | warning: parse errors detected]

// Full source of the matched node
```

### What "Full Symbol" Returns

The complete source text from the node's start byte to end byte — signature, body, and everything in between. The AI gets the exact bytes from the original file, no transformation.

**Doc comments and decorators:** tree-sitter often places doc comments and decorators/attributes as *sibling* nodes preceding the function node, not as children. The symbol resolver must look back at preceding siblings to include these in the extracted range. The extraction range starts at the first preceding doc comment or decorator, and ends at the node's end byte.

---

## Symbol Resolution (`resolve.rs`)

### Algorithm

1. Parse the file with tree-sitter via the appropriate `LanguageBackend`
2. Walk the AST depth-first, maintaining a **scope stack** for parent context:
   - When entering a container structural node (impl block, class, module), push its name onto the scope stack
   - When exiting, pop it
   - The top of the stack provides `parent_context` for any match found
3. Collect all **named nodes** that match the requested symbol. Two categories:
   - **Structural nodes** — where `backend.is_structural_node()` returns `true` AND the node has a `name` child field (identifier). This covers functions, methods, classes, impl blocks.
   - **Data nodes** — structs, enums, interfaces, type aliases, traits, constants. These aren't structural in the Phase 1 pruning sense, but are valid lookup targets for `read_full_symbol`. Data node identification uses **hardcoded tree-sitter node kind strings** in `resolve.rs` (not a trait method on `LanguageBackend`). This keeps the Phase 1 trait focused on skeletonization while allowing `skltn-mcp` to own symbol resolution logic independently. Name extraction uses the tree-sitter `name` field, which is present on all data node types listed below.
4. Name matching is **exact, case-sensitive**. No fuzzy matching, no substring matching.
5. Apply disambiguation logic:

```rust
pub enum ResolveResult {
    Found { source_text: String, match_info: MatchInfo },
    Ambiguous { matches: Vec<MatchInfo> },
    NotFound,
}

pub struct MatchInfo {
    pub name: String,
    pub start_line: usize,  // 1-indexed
    pub end_line: usize,    // 1-indexed
    pub parent_context: Option<String>,  // e.g., "impl UserProfile"
}
```

**Disambiguation rules:**
- 0 matches → `NotFound`
- 1 match → `Found`
- N matches + `start_line` provided → `Found` (match closest to `start_line`)
- N matches + no `start_line` → `Ambiguous`

### Line Indexing Convention

All line numbers at the MCP boundary are **1-indexed** (matching IDE and human conventions). The conversion from tree-sitter's 0-indexed rows happens in `resolve.rs`:

- **Outgoing** (response to AI): `tree_sitter_row + 1`
- **Incoming** (`start_line` parameter from AI): `mcp_line - 1` before comparing to tree-sitter positions

### Data Node Types Per Language

| Language | Data nodes resolvable by `read_full_symbol` |
|---|---|
| Rust | `struct_item`, `enum_item`, `trait_item`, `type_item`, `const_item`, `static_item` |
| Python | (no additional data nodes — `class_definition` and `function_definition` are already structural nodes) |
| TypeScript | `interface_declaration`, `type_alias_declaration`, `enum_declaration` |
| JavaScript | (no additional data nodes — `class_declaration` and `function_declaration` are already structural nodes) |

> Note: Classes in Python/JS/TS are structural nodes in Phase 1 and will already be found via the structural node branch of the algorithm. They do not need separate data node entries. All data node types listed above have a `name` field in their tree-sitter grammar, enabling consistent name extraction.

---

## Path Security (`resolve.rs`)

Every tool that accepts a path parameter validates it before use:

```rust
pub fn resolve_safe_path(root: &Path, relative: &str) -> Result<PathBuf, McpError> {
    let joined = root.join(relative);
    let canonical_root = root.canonicalize().map_err(|_| McpError::InvalidRoot)?;
    let canonical_candidate = joined.canonicalize().map_err(|_| McpError::FileNotFound)?;

    if !canonical_candidate.starts_with(&canonical_root) {
        return Err(McpError::PathOutsideRoot);
    }

    Ok(canonical_candidate)
}
```

Error types are **`skltn-mcp`-local** (defined in `error.rs`), not modifications to `skltn-core`'s `SkltnError`. Phase 1's error type stays focused on skeletonization errors. Phase 2 defines its own `McpError` enum for MCP-specific failure modes:

```rust
pub enum McpError {
    InvalidRoot,
    FileNotFound,
    PathOutsideRoot,
    UnsupportedLanguage,
    SymbolNotFound,
    Core(skltn_core::SkltnError),  // Wraps Phase 1 errors
}
```

- `canonicalize()` resolves symlinks and `..` segments
- `starts_with()` check ensures the resolved path is within the repo root
- `McpError::PathOutsideRoot` maps to a content response: `"Path is outside the repository root"` — no information about the actual root path is leaked
- `McpError::FileNotFound` maps to: `"File not found: {path}"` (the relative path as provided, not the resolved path)

---

## Error Handling (`error.rs`)

### Protocol Errors (MCP Error Responses)

Reserved for fundamentally broken requests. Rare in practice.

| Condition | Error Code | Message |
|---|---|---|
| Malformed parameters (wrong types, missing required fields) | `InvalidParams` | `"Missing required parameter: file"` |
| Internal server crash (panic, unexpected failure) | `InternalError` | `"Internal error: {details}"` |
| Root path invalid / inaccessible at startup | Server fails to start | stderr message, exit code 1 |

### Content Responses (Successful MCP Responses)

Operational feedback the AI can reason about and self-correct.

| Condition | Tool(s) | Response Content |
|---|---|---|
| File not found | `read_skeleton`, `read_full_symbol` | `"File not found: {path}"` |
| Directory not found | `list_repo_structure` | `"Directory not found: {path}"` |
| Path is a file (expected directory) | `list_repo_structure` | `"Path is a file, not a directory: {path}. Use read_skeleton to inspect it."` |
| Path traversal attempt | All | `"Path is outside the repository root"` |
| File too large (>10 MB) | `read_skeleton`, `read_full_symbol` | `"File too large: {path} ({size} bytes, limit is 10 MB)"` |
| Unsupported language | `read_skeleton`, `read_full_symbol` | `"Unsupported language for file: {path}. Supported: .rs, .py, .ts, .js"` |
| Symbol not found | `read_full_symbol` | `"Symbol '{name}' not found in {path}"` |
| Ambiguous symbol | `read_full_symbol` | Disambiguation list with parent context and line ranges |
| No supported files | `list_repo_structure` | `"No supported source files (.rs, .py, .ts, .js) found in {path}"` |
| File has syntax errors | `read_skeleton`, `read_full_symbol` | Partial result returned (tree-sitter error tolerance), `warning: parse errors detected` in metadata header |

### Response Structure Convention

Every successful tool response with code content follows:

1. **Metadata line** — single `[bracketed]` line with key stats
2. **Blank line separator**
3. **Content** — the actual code or file listing

For error/informational content responses (file not found, symbol not found, etc.), the metadata line is omitted — just the plain message. The `[` prefix allows the AI to instantly distinguish payloads from messages.

---

## Testing Strategy

### Test Categories

| Category | What It Validates |
|---|---|
| Tool registration | All three tools are registered and discoverable via MCP |
| `list_repo_structure` basics | Returns correct tree format with byte sizes and languages |
| `list_repo_structure` with `max_depth` | Depth limiting works correctly |
| `list_repo_structure` edge cases | Empty directory, file path passed as directory, path traversal |
| `read_skeleton` full file | Files ≤2k tokens returned in full with correct metadata |
| `read_skeleton` skeletonized | Files >2k tokens returned as skeletons with compression stats |
| `read_skeleton` edge cases | File not found, unsupported language, syntax errors |
| Budget Guard | Token threshold correctly determines full vs skeleton |
| `read_full_symbol` single match | Returns full source text with metadata |
| `read_full_symbol` ambiguous | Returns disambiguation list with parent context |
| `read_full_symbol` with `start_line` | Resolves ambiguity by selecting closest match |
| `read_full_symbol` data nodes | Structs, enums, interfaces, traits resolvable |
| `read_full_symbol` not found | Returns clear not-found message |
| Symbol resolution scope stack | `parent_context` correctly tracks containing scope |
| Path security | Path traversal attempts blocked, no info leakage |
| Line indexing | 1-indexed lines in all MCP responses and parameters |
| Content vs protocol errors | Correct error type for each failure mode |
| `spawn_blocking` | CPU-bound operations don't block the transport loop |

### Test Fixtures

Phase 2 tests will use the same fixture files from Phase 1 (`fixtures/rust/`, `fixtures/python/`, etc.) for symbol resolution and skeletonization tests. Additional fixtures may be added for MCP-specific edge cases (e.g., files near the 2k token boundary for Budget Guard testing).

### Integration Testing

MCP tool calls can be tested by constructing `rmcp` request objects directly (without a live stdio transport). Each tool handler takes the server state and request parameters, returning a response — these are unit-testable.

---

## Success Criteria (Phase 2)

| Metric | Target |
|---|---|
| Tool coverage | All three tools functional and registered |
| Budget Guard accuracy | Files ≤2k tokens returned in full, >2k skeletonized |
| Symbol resolution | Single match, disambiguation, and not-found all handled correctly |
| Path security | Zero path traversal vulnerabilities |
| Error handling | Protocol errors for broken requests, content responses for operational feedback |
| Latency | Tool responses <100ms for single-file operations on typical repos |

---

## Amendment: Cache-Aware Budget Guard (2026-03-17)

### Problem: Prompt Caching Inverts the Cost Equation

Anthropic's prompt caching gives a 90% discount on repeated input tokens. This fundamentally changes the economics of skeletonization:

- A 4,000-token file **cached** costs ~400 effective tokens (full price on first read, 90% off on subsequent reads)
- The same file **skeletonized** to 1,000 tokens costs 1,000 tokens every time (skeleton is a different token sequence, won't hit the cache from a previous full-file read)
- **Skeletonizing a cached file is 2.5x more expensive than serving it full**

The current Budget Guard in `budget.rs` is token-count-only with zero cache awareness. It always skeletonizes files >2k tokens, which actively harms cost when those files are already in the provider's prompt cache.

### Solution: Session-Aware Budget Guard with CacheHint

Add an in-memory LRU tracker of files recently served in full. If a file was served full within the current session, serve it full again (it's likely cached by the provider). If it's the first request for a file, apply the existing token threshold logic.

#### CacheHint Enum

```rust
pub enum CacheHint {
    /// No prior information — cold start, use token threshold heuristic
    Unknown,
    /// File was served full recently in this session — likely cached by provider
    RecentlyServed,
    /// Phase 3 integration: obs proxy confirmed cache_read_input_tokens > 0
    CacheConfirmed,
    /// Phase 3 integration: obs data is stale (>5min since last cache hit)
    CacheExpired,
}
```

`CacheConfirmed` and `CacheExpired` are **reserved for Phase 3 integration** — they are defined in the enum but not produced by any Phase 2 code. This provides a clean extension point without requiring Phase 2 changes when Phase 3 lands.

#### Updated BudgetDecision Logic

```rust
pub fn should_skeletonize(
    source: &str,
    tokenizer: &CoreBPE,
    hint: CacheHint,
) -> BudgetDecision {
    let token_count = tokenizer.encode_ordinary(source).len();

    match hint {
        // File is likely cached — serve full regardless of size
        CacheHint::RecentlyServed | CacheHint::CacheConfirmed => {
            BudgetDecision::ReturnFull { original_tokens: token_count }
        }
        // No cache info or cache expired — fall back to token threshold
        CacheHint::Unknown | CacheHint::CacheExpired => {
            if token_count > TOKEN_THRESHOLD {
                BudgetDecision::Skeletonize { original_tokens: token_count }
            } else {
                BudgetDecision::ReturnFull { original_tokens: token_count }
            }
        }
    }
}
```

#### Session Tracker

The MCP server gains a lightweight `SessionTracker` that records which files have been served full:

- **Data structure:** `HashMap<PathBuf, Instant>` — maps file path to the timestamp of last full-file serve
- **On `read_skeleton` returning full content:** Record the file path and current time
- **On `read_skeleton` query:** Look up the file; if present, produce `CacheHint::RecentlyServed`; if absent, produce `CacheHint::Unknown`
- **No TTL eviction needed in Phase 2:** The MCP server process lifetime equals the session lifetime. The map is naturally bounded by the number of unique files accessed in a session.
- **Memory bound:** Even for a 10,000-file repo where every file is accessed, the map costs ~1 MB. No eviction policy needed.

#### Server State Change

The `SkltnServer` struct gains a `session_tracker: SessionTracker` field. This is the **only stateful addition** — the server was previously fully stateless. The tracker is scoped to the process lifetime (one MCP session).

#### `read_skeleton` Response Metadata Update

When a file is served full due to cache hint (rather than being under the token threshold), the metadata header indicates this:

```
[file: src/engine.rs | language: rust | tokens: 4,821 | full file (cache-aware)]
```

The `(cache-aware)` suffix tells the AI (and the user inspecting logs) that this file was served full because of caching economics, not because it was small.

#### First-Read Behavior: Skeleton Still Valuable

On the **first** read of a large file (no cache hint), skeletonization remains the correct default:
- The file isn't cached yet, so full-price applies
- The skeleton gives the AI a structural overview at reduced cost
- If the AI needs the full file, it requests it via `read_full_symbol` or a second `read_skeleton` call (which will now be served full via `RecentlyServed` — but note: only if the first call served it full)

**Important nuance:** The first `read_skeleton` call for a large file still returns the skeleton (not full). The `RecentlyServed` hint only applies when the file was previously served **full** (either because it was under the threshold, or because it was served full on a subsequent cache-aware call). The skeleton itself is not tracked as "recently served full" because the skeleton token sequence differs from the full file and wouldn't benefit from the provider's cache.

#### Interaction with `read_full_symbol`

`read_full_symbol` always returns full source text for a specific symbol. It does **not** update the session tracker because:
- It returns a fragment, not the full file
- The fragment's token sequence doesn't match the full file's cache entry
- Tracking fragments would add complexity without cache benefit

#### Expected Cache Accuracy

- **Option A alone (this amendment): ~80%** — covers repeated file reads within a single MCP session
- **Option A + Phase 3 integration (future): ~98%** — heuristic for cold-start, actual cache data for steady-state

### Migration

This is a **backward-compatible change** to the Budget Guard. The existing behavior (skeletonize >2k tokens) is preserved for first reads. The only behavioral change is: files served full in the current session are served full again on subsequent reads.

No changes to `read_full_symbol` or `list_repo_structure`.

---

## Out of Scope (Phase 2)

- Token counting observability / cost tracking (Phase 3)
- Web Dashboard (Phase 4)
- Directory skeletonization in a single tool call (AI skeletons files individually)
- Fuzzy symbol matching or path correction
- ~~Caching of directory structure or file contents~~ (Session tracker added for cache-aware Budget Guard — see amendment above)
- Configuration of the token threshold (it's a constant)
- Additional languages beyond Phase 1's four
- HTTP/SSE transport (stdio only)
- Phase 3 `CacheHint` integration (enum variants defined but not wired)
