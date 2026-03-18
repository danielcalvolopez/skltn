# skltn

High-performance Rust toolchain for AI context window optimization via AST-based skeletonization and observability.

"Skltn lets Claude read your entire codebase architecture without filling up its context window."

## How It Works

When Claude needs to read a file, skltn's MCP server intercepts the request:

- **Small files** (≤2000 tokens) are served in full — no changes.
- **Large files** (>2000 tokens) are skeletonized via AST parsing — function signatures, type definitions, and doc comments are preserved while implementation bodies are collapsed.
- When Claude needs a specific function body, it calls `read_full_symbol` to drill down into just that symbol.

The observability proxy sits between Claude Code and the Anthropic API, recording every request so you can see real-time cost, token usage, and savings on a live dashboard.

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
                    └── ~/.skltn/
```

### Crates

| Crate | Description |
| --- | --- |
| **skltn-core** | AST-based skeletonization engine. Supports Rust, Python, TypeScript, and JavaScript. |
| **skltn-cli** | Standalone CLI for skeletonizing files offline. |
| **skltn-mcp** | MCP server providing `read_skeleton`, `read_full_symbol`, and `list_repo_structure` tools. Tracks savings and drilldown events to JSONL. |
| **skltn-obs** | HTTP proxy between Claude Code and Anthropic API. Records token usage and cost, watches savings files, and serves a real-time dashboard via WebSocket. |

### MCP Tools

| Tool | Description |
| --- | --- |
| `list_repo_structure` | Directory tree with file sizes and language detection. |
| `read_skeleton` | Returns full file if ≤2000 tokens, otherwise returns the skeletonized AST view. |
| `read_full_symbol` | Drills into a specific symbol by name, with line number disambiguation for overloads. |

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

## Dashboard

The dashboard connects over WebSocket and updates in real-time.

**Metrics bar** — top-level indicators at a glance:

| Metric | Description |
| --- | --- |
| Files Explored | Number of files read via skeletonization |
| Context Density | Percentage of original tokens used after skeletonization (27% = 73% reduction) |
| Drilldowns | Number of `read_full_symbol` calls |
| Tokens Used | Total tokens consumed vs context window limit |

**Sidebar** — cost tracking and model breakdown:

| Metric | Description |
| --- | --- |
| Session Cost | Total USD spent on API calls this session |
| Cost Saved | Estimated USD saved by skeletonization |
| Model Breakdown | Per-model token usage (Opus, Sonnet, Haiku) |

**Exploration Hero** — shows the exploration multiplier (how many more files you can explore with skeletonization vs reading files in full).

**Token chart** — cumulative token usage with two lines: **With Skltn** (actual) vs **Without Skltn** (projected). The gap is your savings.

**Request table** — per-request details with model, token counts, and cost.

## Data Storage

All session data is stored in `~/.skltn/`:

| File | Written by | Contents |
| --- | --- | --- |
| `usage.jsonl` | skltn-obs | One JSON line per API request (model, tokens, cost) |
| `savings.jsonl` | skltn-mcp | One JSON line per skeletonization (file, original vs skeleton tokens) |
| `drilldowns.jsonl` | skltn-mcp | One JSON line per `read_full_symbol` call |

Files are session-scoped — truncated on proxy startup.

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

## Supported Languages

| Language | Extensions |
| --- | --- |
| Rust | `.rs` |
| Python | `.py` |
| TypeScript | `.ts` |
| JavaScript | `.js` |
