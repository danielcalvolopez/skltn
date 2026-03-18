# skltn

High-performance Rust toolchain for AI context window optimization via AST-based skeletonization and observability.

"Skltn lets Claude read your entire codebase architecture without filling up its context window."

## How It Works

When Claude needs to read a file, skltn's MCP server intercepts the request:

- **Small files** (≤2000 tokens) are served in full — no changes.
- **Large files** (>2000 tokens) are skeletonized via AST parsing — function signatures, type definitions, and doc comments are preserved while implementation bodies are collapsed.
- When Claude needs a specific function body, it calls `read_full_symbol` to drill down into just that symbol.

The result: Claude explores 5-15x more codebase within the same context window.

## Architecture

```
Claude Code ──► skltn-obs proxy (port 8080) ──► Anthropic API
                    │
                    ├── Dashboard (localhost:8080/dashboard)
                    │       ▲
                    │       │ WebSocket (usage + savings)
                    │       │
                    ├── usage.jsonl ◄── proxy records API usage
                    │
skltn-mcp ──────────┼── savings.jsonl ◄── MCP records skeletonization savings
                    │
                    ├── drilldowns.jsonl ◄── MCP records symbol drilldowns
                    │
                    └── ~/.skltn/cache/<project-hash>/ ◄── persistent skeleton cache
```

### Crates

| Crate | Description |
| --- | --- |
| **skltn-core** | AST-based skeletonization engine. Supports Rust, Python, TypeScript, JavaScript, TSX, and JSX. |
| **skltn-cli** | Standalone CLI for skeletonizing files offline. |
| **skltn-mcp** | MCP server providing `read_skeleton`, `read_full_symbol`, `list_repo_structure`, and `restore_session` tools. Manages the skeleton cache, session manifest, session tracker, and savings/drilldown recording. |
| **skltn-obs** | HTTP proxy between Claude Code and Anthropic API. Records token usage and cost, watches savings files, and serves a real-time dashboard via WebSocket. |

## Skeletonization Engine

The engine uses tree-sitter to parse source files into ASTs, then selectively collapses implementation bodies while preserving structural information.

### What is preserved

- Function and method **signatures** (name, parameters, return type)
- **Type definitions** — structs, enums, interfaces, type aliases
- **Class and trait declarations** (container structure is recursed into)
- **Doc comments** — `///`, `//!`, `/** */`, and Python docstrings
- **Decorators and attributes** — `#[derive(...)]`, `@decorator`
- **Import statements** and top-level constants
- **Indentation** — skeleton output is valid, readable code

### What is collapsed

Function and method bodies are replaced with a language-appropriate placeholder and a line count:

**Rust:**
```rust
pub fn process_data(input: &[u8]) -> Result<Output, Error> {
    todo!()    // [skltn: 45 lines hidden]
}
```

**Python:**
```python
def process_data(self, input: bytes) -> Output:
    """Process raw input bytes into structured output."""
    pass    # [skltn: 32 lines hidden]
```

**TypeScript/JavaScript:**
```typescript
export function processData(input: Uint8Array): Output {
    throw new Error("not implemented")    // [skltn: 28 lines hidden]
}
```

Python docstrings are extracted and preserved before the `pass` placeholder. Container nodes (classes, impl blocks, trait definitions, modules) are recursed into — only leaf function bodies are collapsed.

### Depth limiting

`SkeletonOptions { max_depth }` controls how many nesting levels are skeletonized. Useful for deeply nested codebases where inner implementation details add noise.

## MCP Tools

| Tool | Description |
| --- | --- |
| `list_repo_structure` | Directory tree of supported source files with byte sizes and language detection. Accepts optional `path` (subdirectory) and `max_depth` parameters. |
| `read_skeleton` | Returns full file if ≤2000 tokens, otherwise the skeletonized AST view. Cache-aware — avoids re-skeletonizing files already in the provider's prompt cache. |
| `read_full_symbol` | Drills into a specific symbol by name. Supports disambiguation via `start_line` for overloads. Returns the full source including doc comments and decorators. |
| `restore_session` | Restores context from the previous session. Returns a summary of files read last session with change annotations (unchanged/modified/removed). Use `load=true` to batch-load file contents, `only_changed=true` to filter to modified files only. |

### Recommended workflow

```
restore_session → reload context from previous session (if any)
         ↓
list_repo_structure → understand project shape
         ↓
read_skeleton → read structural overview of relevant files
         ↓
read_full_symbol → drill into specific functions when needed
```

## Token Budget System

The budget system decides whether to skeletonize each file based on token count and prompt cache state.

### Budget decision flow

```
read_skeleton(file)
  ├─ count tokens in file
  ├─ get cache hint from SessionTracker
  │
  ├─ tokens ≤ 2000 → ReturnFull (always)
  ├─ tokens > 2000 + hint = RecentlyServed/CacheConfirmed → ReturnFull (cache-aware)
  └─ tokens > 2000 + hint = Unknown/CacheExpired → Skeletonize
```

### Cache-aware serving

When a large file has been served in full previously in the same session, it is likely still in the LLM provider's prompt cache. Re-sending the full file is cheaper than sending a different skeleton (which would miss the cache). The `SessionTracker` records which files have been served full and provides `CacheHint` values:

| Hint | Meaning | Decision for >2K files |
| --- | --- | --- |
| `Unknown` | First read, no prior info | Skeletonize |
| `RecentlyServed` | Served full earlier this session | Return full (cache-aware) |
| `CacheConfirmed` | Provider cache hit confirmed | Return full |
| `CacheExpired` | >5 min since last serve | Skeletonize |

Files under the 2000-token threshold are always served full regardless of hint.

## Cross-Session Skeleton Cache

Skeletons are persisted to disk so that subsequent sessions don't re-parse unchanged files.

### Storage

```
~/.skltn/cache/<project-hash>/
  ├── src__components__UserProfile.tsx.json
  ├── src__lib__auth.rs.json
  └── ...
```

- **Project hash**: SHA-256 of the canonicalized project root path (16 hex chars)
- **One JSON file per cached skeleton**, containing: `content_hash`, `mtime_secs`, `original_tokens`, `skeleton_tokens`, `has_parse_errors`, and `skeleton` text

### Two-tier cache invalidation

1. **Fast path (mtime check)** — compare file's mtime against the cached value. If identical, return the cached skeleton immediately. This is a single stat call.
2. **Slow path (content hash)** — if mtime differs (e.g. after `git checkout`), hash the file contents and compare against `content_hash`. If content is identical, update the stored mtime and return the cached skeleton. If content differs, re-skeletonize.

### Cache lifecycle

- **Startup cleanup**: on server initialization, entries for deleted files are removed
- **Cache hits**: skip AST parsing and skeletonization entirely — the two most expensive operations
- **Cache misses**: skeletonize normally, write the result to cache for future sessions
- The cache only operates in the `Skeletonize` branch — files served full bypass it entirely

## Session Manifest & Cross-Session Context Restoration

The session manifest tracks which files the LLM reads during each session, enabling fast context restoration on the next session.

### How it works

1. Every `read_skeleton` call records the file path in the current session's manifest
2. The manifest is flushed to disk periodically (every 5s) and on server shutdown
3. On the next session start, the previous manifest is rotated to `manifest.previous.json`
4. Calling `restore_session` reads the previous manifest and reports what changed

### Storage

```
~/.skltn/cache/<project-hash>/
  ├── manifest.json              ◄── current session's file list
  ├── manifest.previous.json     ◄── previous session's file list
  ├── src__main.rs.json          ◄── skeleton cache entries
  └── ...
```

### restore_session modes

| Mode | Parameters | Description |
| --- | --- | --- |
| **TOC (default)** | `load=false` | Summary table: file, language, estimated tokens, change status (unchanged/modified/removed) |
| **Load all** | `load=true` | Batch-loads skeleton/full content for all previous session files in a single round trip |
| **Load changed only** | `load=true, only_changed=true` | Only loads files that were modified since last session — skips unchanged files to save tokens |

Load mode respects a 50,000-token budget. If the total exceeds the budget, remaining files are omitted with a truncation notice.

### Change detection

Each file from the previous manifest is checked against its current state:

- **unchanged** — content hash matches the skeleton cache entry
- **modified** — file exists but content has changed since last session
- **removed** — file no longer exists on disk

## Observability Dashboard

The dashboard connects over WebSocket and updates in real-time.

**Metrics bar** — top-level indicators:

| Metric | Description |
| --- | --- |
| Files Explored | Unique files read via `read_skeleton` (both full and skeletonized) |
| Context Density | Ratio of skeleton tokens to original tokens (21% = 79% compression) |
| Drilldowns | Number of `read_full_symbol` calls |
| API Tokens | Cumulative tokens across all API requests in the session |
| Skeleton Tokens | Total tokens occupied by skeletons delivered to the LLM |

**Sidebar** — cost tracking:

| Metric | Description |
| --- | --- |
| Session Cost | Total USD spent on API calls this session |
| Cost Saved | Estimated USD saved by skeletonization (`saved_tokens × dominant_model_input_rate`) |
| Model Breakdown | Per-model cost (Opus, Sonnet, Haiku) |

**Exploration Hero** — the headline metric: how many times more codebase you explored vs reading files in full. A 5x multiplier means you fit 5x more structural information into the same context window.

**Token chart** — cumulative token usage over time with two lines: actual (with skltn) vs projected (without skltn).

**Request table** — per-request details with model, token counts, and cost.

## Data Storage

All session data is stored in `~/.skltn/`:

| Path | Written by | Contents | Lifecycle |
| --- | --- | --- | --- |
| `usage.jsonl` | skltn-obs | One JSON line per API request (model, tokens, cost) | Truncated on proxy startup |
| `savings.jsonl` | skltn-mcp | One JSON line per file read (original vs skeleton tokens) | Truncated on proxy startup |
| `drilldowns.jsonl` | skltn-mcp | One JSON line per `read_full_symbol` call | Truncated on proxy startup |
| `cache/<project-hash>/` | skltn-mcp | Persistent skeleton cache (one JSON file per cached skeleton) | Persists across sessions, stale entries cleaned on startup |
| `cache/<project-hash>/manifest.json` | skltn-mcp | Current session's file manifest (files read by the LLM) | Rotated to `manifest.previous.json` on next session start |
| `cache/<project-hash>/manifest.previous.json` | skltn-mcp | Previous session's file manifest | Overwritten on each session start |

## Supported Languages

| Language | Extensions | Structural Nodes | Docstring Handling |
| --- | --- | --- | --- |
| Rust | `.rs` | functions, impl blocks, traits, modules, closures | `///` and `//!` doc comments preserved |
| Python | `.py` | functions, classes | Leading docstrings extracted and preserved before placeholder |
| TypeScript | `.ts` | functions, methods, classes, abstract classes | `/** */` JSDoc comments preserved |
| JavaScript | `.js` | functions, methods, classes, arrow functions | `/** */` JSDoc comments preserved |
| TSX | `.tsx` | Same as TypeScript | Same as TypeScript |
| JSX | `.jsx` | Same as JavaScript | Same as JavaScript |

## Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- [Node.js](https://nodejs.org/) >= 18 + [pnpm](https://pnpm.io/)
- [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) (`claude`)
- [just](https://github.com/casey/just) (optional, for build shortcuts)

## Local Setup

### 1. Build everything

```bash
# Build the dashboard frontend + all Rust crates
just build

# Or manually:
cd crates/skltn-obs/dashboard && pnpm install && pnpm build && cd ../../..
cargo build --workspace
```

### 2. Start the observability proxy

```bash
# Terminal 1
RUST_LOG=debug cargo run -p skltn-obs -- --port 8080
```

This starts the proxy on `localhost:8080` and serves the dashboard at [localhost:8080/dashboard](http://localhost:8080/dashboard).

### 3. Register the MCP server with Claude Code

```bash
# Terminal 2 — run from the project root
claude mcp add skltn-mcp -- cargo run -p skltn-mcp -- "$(pwd)"
```

This only needs to be done once per project. Verify it was added:

```bash
claude mcp list
```

### 4. Start Claude Code through the proxy

```bash
# Terminal 2
ANTHROPIC_BASE_URL=http://localhost:8080 claude
```

All API requests now flow through the proxy. The dashboard shows real-time metrics.

### 5. Open the dashboard

Navigate to [http://localhost:8080/dashboard](http://localhost:8080/dashboard) in your browser.

## Using with Other Projects

skltn works with any project. You can run it from anywhere using the built binaries.

### Option A: Use absolute paths

```bash
# Set this to wherever you cloned skltn
SKLTN=/path/to/skltn

# Terminal 1 — start the proxy
$SKLTN/target/release/skltn-obs --port 8080

# Terminal 2 — cd to your project and register the MCP server
cd ~/my-other-project
claude mcp add skltn-mcp -- $SKLTN/target/release/skltn-mcp "$(pwd)"

# Start Claude through the proxy
ANTHROPIC_BASE_URL=http://localhost:8080 claude
```

### Option B: Install to PATH (recommended)

```bash
# From the skltn repo — installs binaries to ~/.cargo/bin/
cargo install --path crates/skltn-obs
cargo install --path crates/skltn-mcp

# Now use from any project
cd ~/my-other-project
skltn-obs --port 8080 &
claude mcp add skltn-mcp -- skltn-mcp "$(pwd)"
ANTHROPIC_BASE_URL=http://localhost:8080 claude
```

The MCP registration is per-project in Claude Code, so you need to run `claude mcp add` once for each project you want to use skltn with.

## Commands

```bash
# Build (frontend + Rust)
just build

# Build dashboard only
just build-ui

# Dev mode (proxy on :8080)
just dev

# Test
cargo test --workspace

# Lint
cargo clippy --all-targets --all-features

# CLI (standalone skeletonization)
cargo run -p skltn-cli -- <PATH>

# Dashboard dev server (hot reload, proxies WS to port 8080)
cd crates/skltn-obs/dashboard && pnpm dev
```
