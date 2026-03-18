# skltn

High-performance Rust toolchain for AI context window optimization via AST-based skeletonization and observability.

Skltn lets Claude read your entire codebase architecture without filling up its context window.

## Quick Start

### Install

```bash
curl -fsSL https://raw.githubusercontent.com/danielcalvolopez/skltn/main/install.sh | sh
```

This installs `skltn`, `skltn-mcp`, and `skltn-obs` to `~/.skltn/bin/` and adds it to your PATH.

Requires [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) (`claude`).

### Use

```bash
cd ~/my-project
skltn start
```

That's it. One command:
1. Starts the observability proxy in the background
2. Registers the MCP server with Claude Code for the current directory
3. Launches Claude Code with all requests flowing through the proxy

Open [localhost:8080/dashboard](http://localhost:8080/dashboard) to see real-time token savings.

### Other commands

```bash
skltn stop           # Stop the background proxy
skltn status         # Show proxy info
skltn start --no-obs # MCP-only mode (no proxy/dashboard)
```

## How It Works

When Claude reads a file through skltn's MCP server:

- **Small files** (≤2000 tokens) are served in full — no changes.
- **Large files** (>2000 tokens) are skeletonized via AST parsing — function signatures, type definitions, and doc comments are preserved while implementation bodies are collapsed.
- When Claude needs a specific function body, it calls `read_full_symbol` to drill down into just that symbol.

The result: Claude explores 5-15x more codebase within the same context window.

### Before and after

A 200-line Rust file becomes:

```rust
pub fn process_data(input: &[u8]) -> Result<Output, Error> {
    todo!()    // [skltn: 45 lines hidden]
}
```

Signatures, types, doc comments, and imports are preserved. Only function bodies are collapsed.

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
| **skltn** | Unified CLI — `start`, `stop`, `status`, `skeletonize` subcommands. |
| **skltn-core** | AST-based skeletonization engine. Supports Rust, Python, TypeScript, JavaScript, TSX, and JSX. |
| **skltn-mcp** | MCP server providing `read_skeleton`, `read_full_symbol`, `list_repo_structure`, and `restore_session` tools. |
| **skltn-obs** | HTTP proxy between Claude Code and Anthropic API. Records token usage and cost, serves a real-time dashboard. |

## MCP Tools

| Tool | Description |
| --- | --- |
| `list_repo_structure` | Directory tree of supported source files with byte sizes and language detection. |
| `read_skeleton` | Returns full file if ≤2000 tokens, otherwise the skeletonized AST view. Cache-aware. |
| `read_full_symbol` | Drills into a specific symbol by name. Supports disambiguation via `start_line`. |
| `restore_session` | Restores context from the previous session with change annotations (unchanged/modified/removed). |

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

## Supported Languages

| Language | Extensions | Docstring Handling |
| --- | --- | --- |
| Rust | `.rs` | `///` and `//!` doc comments preserved |
| Python | `.py` | Leading docstrings extracted and preserved before placeholder |
| TypeScript | `.ts`, `.tsx` | `/** */` JSDoc comments preserved |
| JavaScript | `.js`, `.jsx` | `/** */` JSDoc comments preserved |

## Skeletonization Details

The engine uses tree-sitter to parse source files into ASTs, then selectively collapses implementation bodies while preserving structural information.

**Preserved:** function/method signatures, type definitions, class/trait declarations, doc comments, decorators/attributes, imports, top-level constants, indentation.

**Collapsed:** function and method bodies, replaced with a language-appropriate placeholder and line count.

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

Container nodes (classes, impl blocks, trait definitions, modules) are recursed into — only leaf function bodies are collapsed.

### Depth limiting

`skltn skeletonize --max-depth <N>` controls how many nesting levels are skeletonized. Useful for deeply nested codebases where inner implementation details add noise.

## Token Budget System

The budget system decides whether to skeletonize each file based on token count and prompt cache state.

```
read_skeleton(file)
  ├─ tokens ≤ 2000 → return full (always)
  ├─ tokens > 2000 + recently served → return full (cache-aware)
  └─ tokens > 2000 + not cached → skeletonize
```

When a large file has been served in full previously in the same session, it is likely still in the LLM provider's prompt cache. Re-sending the full file is cheaper than sending a different skeleton (which would miss the cache).

## Cross-Session Cache

Skeletons are persisted to `~/.skltn/cache/<project-hash>/` so subsequent sessions don't re-parse unchanged files.

**Two-tier invalidation:**
1. **Fast path** — mtime check (single stat call)
2. **Slow path** — content hash comparison (handles `git checkout` changing mtime without changing content)

## Session Manifest

The session manifest tracks which files Claude reads during each session, enabling fast context restoration on the next session via `restore_session`.

| Mode | Description |
| --- | --- |
| **TOC** (default) | Summary table with change status |
| **Load all** (`load=true`) | Batch-loads all previous session files |
| **Load changed** (`load=true, only_changed=true`) | Only loads modified files |

Load mode respects a 50,000-token budget.

## Observability Dashboard

The dashboard at `localhost:8080/dashboard` connects over WebSocket and updates in real-time.

- **Exploration multiplier** — how many times more codebase you explored vs reading files in full
- **Context density** — ratio of skeleton tokens to original tokens (21% = 79% compression)
- **Token chart** — cumulative usage over time: actual (with skltn) vs projected (without)
- **Cost tracking** — session cost, estimated savings, per-model breakdown
- **Request table** — per-request details with model, token counts, and cost

## Alternative Installation

### Install from source

Requires [Rust](https://rustup.rs/) (stable), [Node.js](https://nodejs.org/) >= 18, [pnpm](https://pnpm.io/).

```bash
git clone https://github.com/danielcalvolopez/skltn.git
cd skltn

# Build dashboard frontend + all Rust crates
cd crates/skltn-obs/dashboard && pnpm install && pnpm build && cd ../../..
cargo install --path crates/skltn
cargo install --path crates/skltn-mcp
cargo install --path crates/skltn-obs
```

### Standalone skeletonization

```bash
skltn skeletonize src/main.rs          # Single file
skltn skeletonize src/                 # Entire directory
skltn skeletonize --raw src/main.rs    # No markdown fencing
skltn skeletonize --lang python script # Force language detection
```

## Development

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

# Dashboard dev server (hot reload, proxies WS to port 8080)
cd crates/skltn-obs/dashboard && pnpm dev
```

## Data Storage

All session data is stored in `~/.skltn/`:

| Path | Contents |
| --- | --- |
| `bin/` | Installed binaries |
| `obs.pid` | Running proxy PID and port |
| `obs.log` | Proxy stdout/stderr |
| `usage.jsonl` | API request log (model, tokens, cost) |
| `savings.jsonl` | File read log (original vs skeleton tokens) |
| `drilldowns.jsonl` | Symbol drilldown log |
| `cache/<project-hash>/` | Persistent skeleton cache + session manifests |

## License

MIT
