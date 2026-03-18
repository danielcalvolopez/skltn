# skltn

High-performance Rust toolchain for AI context window optimization via AST-based skeletonization and observability.

"Skltn lets Claude read your entire codebase architecture without filling up its context window."

## Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- [Node.js](https://nodejs.org/) >= 18 + [pnpm](https://pnpm.io/)
- [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) (`claude`)
- [just](https://github.com/casey/just) (optional, for build shortcuts)

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
                    └── ~/.skltn/
```

- **skltn-obs** — HTTP proxy that sits between Claude Code and the Anthropic API. Records token usage, calculates cost, serves a real-time dashboard.
- **skltn-mcp** — MCP server that provides `read_skeleton`, `read_full_symbol`, and `list_repo_structure` tools. Skeletonizes large files to reduce context window usage.
- **skltn-core** — AST-based skeletonization engine (Rust, Python, TypeScript, JavaScript).
- **skltn-cli** — Standalone CLI for skeletonizing files offline.

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

## Dashboard Metrics

| Metric        | Source    | Description                               |
| ------------- | --------- | ----------------------------------------- |
| Session Cost  | obs proxy | Total USD spent on API calls this session |
| Skltn Savings | MCP + obs | Estimated USD saved by skeletonization    |
| Tokens        | obs proxy | Total tokens consumed (input + output)    |
| Tokens Saved  | MCP       | Total tokens avoided via skeletonization  |
| Requests      | obs proxy | Number of API requests                    |

The chart shows cumulative token usage with two lines: **With Skltn** (actual) vs **Without Skltn** (what it would have been). The gap is your savings.

Savings are only recorded when the MCP server skeletonizes a file (>2000 tokens). Small files served in full produce no savings records.

## Using with Other Projects

skltn works with any project, not just itself. You can run it from anywhere using the built binaries.

### Option A: Use absolute paths

```bash
# Set this to wherever you cloned skltn
SKLTN=/path/to/skltn

# Terminal 1 — start the proxy (runs from anywhere)
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
# Build
cargo build --workspace

# Test
cargo test --workspace

# Lint
cargo clippy --all-targets --all-features

# CLI (standalone skeletonization)
cargo run -p skltn-cli -- <PATH>

# Proxy
cargo run -p skltn-obs -- --port 8080

# Dashboard dev server (hot reload, proxies WS to port 8080)
cd crates/skltn-obs/dashboard && pnpm dev
```
