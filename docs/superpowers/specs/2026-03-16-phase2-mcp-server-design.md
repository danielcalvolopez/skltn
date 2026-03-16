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
4. Show directories for structural context, even if they contain no supported files at the current depth
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
3. Detect language from extension. If unsupported → content response: `"Unsupported language for file: {path}. Supported: .rs, .py, .ts, .js"`
4. Read file contents
5. Run `tiktoken-rs` token count on the source
6. **Budget Guard decision:**
   - Token count ≤ 2,000 → return the full file contents (no skeletonization needed)
   - Token count > 2,000 → skeletonize via `skltn-core::SkeletonEngine::skeletonize()` and return the skeleton
7. Run `tiktoken-rs` token count on the output (whether full or skeleton)
8. Return with metadata header

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

- **Tokenizer model:** `cl100k_base` — used by Claude-family models, close enough for budget decisions (±5% margin is acceptable for a threshold check)
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

1. Resolve path and read file (same not-found / unsupported handling as `read_skeleton`)
2. Parse the file with tree-sitter via `skltn-core`'s language backend
3. Walk the AST using the symbol resolution algorithm (see Symbol Resolution section)
4. Handle results:

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

The complete source text from the node's start byte to end byte — signature, body, doc comments, decorators/attributes. Everything. The AI gets the exact bytes from the original file, no transformation.

---

## Symbol Resolution (`resolve.rs`)

### Algorithm

1. Parse the file with tree-sitter via the appropriate `LanguageBackend`
2. Walk the AST depth-first, maintaining a **scope stack** for parent context:
   - When entering a container structural node (impl block, class, module), push its name onto the scope stack
   - When exiting, pop it
   - The top of the stack provides `parent_context` for any match found
3. Collect all **named nodes** that match the requested symbol. Two categories:
   - **Structural nodes** — where `backend.is_structural_node()` returns `true` AND the node has a name child (identifier). This covers functions, methods, classes, impl blocks.
   - **Data nodes** — structs, enums, interfaces, type aliases, traits, constants. These aren't structural in the Phase 1 pruning sense, but are valid lookup targets for `read_full_symbol`.
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
| Python | `class_definition` (when no methods — pure data class), `expression_statement` (top-level assignments) |
| TypeScript | `interface_declaration`, `type_alias_declaration`, `enum_declaration` |
| JavaScript | `variable_declaration` (top-level const/let exports) |

> Note: `class_definition` in Python and `class_declaration` in TS/JS are already structural nodes. They appear here as a reminder that they're valid `read_full_symbol` targets — the AI might want to see the entire class, not just a skeleton.

---

## Path Security (`resolve.rs`)

Every tool that accepts a path parameter validates it before use:

```rust
pub fn resolve_safe_path(root: &Path, relative: &str) -> Result<PathBuf, SkltnError> {
    let joined = root.join(relative);
    let canonical_root = root.canonicalize().map_err(|_| SkltnError::InvalidRoot)?;
    let canonical_candidate = joined.canonicalize().map_err(|_| SkltnError::FileNotFound)?;

    if !canonical_candidate.starts_with(&canonical_root) {
        return Err(SkltnError::PathOutsideRoot);
    }

    Ok(canonical_candidate)
}
```

- `canonicalize()` resolves symlinks and `..` segments
- `starts_with()` check ensures the resolved path is within the repo root
- `SkltnError::PathOutsideRoot` maps to a content response: `"Path is outside the repository root"` — no information about the actual root path is leaked
- `SkltnError::FileNotFound` maps to: `"File not found: {path}"` (the relative path as provided, not the resolved path)

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

## Out of Scope (Phase 2)

- Token counting observability / cost tracking (Phase 3)
- Tauri HUD (Phase 4)
- Directory skeletonization in a single tool call (AI skeletons files individually)
- Fuzzy symbol matching or path correction
- Caching of directory structure or file contents
- Configuration of the token threshold (it's a constant)
- Additional languages beyond Phase 1's four
- HTTP/SSE transport (stdio only)
