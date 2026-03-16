# PRD: Project Skeleton (skltn)

**Version:** 2.0 (Post-Design)
**Last Updated:** 2026-03-16

---

## 1. Executive Summary

"Skeleton" is a developer infrastructure tool that sits between AI agents and source code. It uses Abstract Syntax Tree (AST) parsing via tree-sitter to strip implementation details while preserving architectural signatures. This extends the effective context window of models like Claude 4.6 and reduces API costs significantly.

The project is delivered in 4 independent phases, each producing a standalone, testable deliverable. All 4 phases are fully designed and planned before any implementation begins.

---

## 2. Technical Stack

| Component      | Technology                                                          |
| -------------- | ------------------------------------------------------------------- |
| Language       | Rust (latest stable)                                                |
| Parsing        | tree-sitter with per-language grammar crates                        |
| Protocol       | Model Context Protocol (MCP) via `rmcp` crate                       |
| Frontend (HUD) | Tauri (Rust backend + React/TS frontend)                            |
| Observability  | `axum` (reverse proxy), `reqwest` (upstream HTTPS), `time` (timestamps) |
| CLI            | `clap` (args), `ignore` (filesystem), `is-terminal` (TTY detection) |
| Testing        | `insta` (snapshot testing)                                          |

---

## 3. Core Features & User Stories

### A. The Skeleton Engine (Phase 1)

**Requirement:** Parse source files using tree-sitter and produce syntactically valid skeletons that preserve architectural signatures while stripping implementation details.

**Requirement:** Support Rust (`.rs`), Python (`.py`), TypeScript (`.ts`), and JavaScript (`.js`).

> **PRD Deviation:** The original PRD specified Solidity (`.sol`) instead of JavaScript. This was changed because JS/TS coverage is more immediately useful and shares grammar infrastructure. Solidity will be added in a future phase as a standalone `SolidityBackend`.

**User Story:** As a dev, I want Claude to see the "map" of my 2,000-line file in under 200 tokens.

### B. The Smart MCP Server (Phase 2)

**Requirement:** Implement MCP tools: `list_repo_structure`, `read_skeleton`, and `read_full_symbol` (formerly `hydrate_context`).

**Requirement:** Add a "Budget Guard" — if a file is >2k tokens, automatically force `read_skeleton`.

**User Story:** As an AI agent, I want to see the skeleton first, then request the full body of only the specific function I need.

### C. Observability Layer (Phase 3)

**Requirement:** Intercept Anthropic API responses and log `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, and `cache_read_input_tokens`.

**Requirement:** Create a `CostTracker` struct that calculates actual cost per request and tracks cumulative session cost.

> **PRD Deviation:** The original PRD specified "calculates savings." During Phase 3 design, this was narrowed to actual cost tracking only. Calculating savings would require knowing what the request *would have looked like* without skeletonization — a counterfactual the proxy cannot observe. The proxy reports what actually happened.

### D. Real-Time Token HUD (Phase 4)

**Requirement:** Tauri app with floating window mode, context usage visualization, and "money saved" odometer.

**Requirement:** Stream metrics via local WebSocket from the observability layer.

**User Story:** As a user, I want to see exactly how much money and context I am saving in real-time.

---

## 4. Architecture Overview

### Cargo Workspace

```
skltn/
├── Cargo.toml                  # Workspace root
├── crates/
│   ├── skltn-core/             # Library — Skeleton Engine (Phase 1)
│   ├── skltn-cli/              # Binary — CLI wrapper (Phase 1)
│   ├── skltn-mcp/              # Binary — MCP server (Phase 2)
│   ├── skltn-obs/              # Binary — Observability proxy (Phase 3)
│   └── skltn-hud/              # Tauri app (Phase 4)
├── fixtures/                   # Test fixtures per language
├── docs/
│   └── superpowers/
│       ├── specs/              # Design specifications per phase
│       └── plans/              # Implementation plans per phase
└── PRD.md
```

### Phase Dependencies

```
Phase 1 (Skeleton Engine) ← standalone
Phase 2 (MCP Server) ← depends on skltn-core
Phase 3 (Observability) ← standalone (no dependency on skltn-core)
Phase 4 (Tauri HUD) ← depends on skltn-obs (WebSocket consumer)
```

---

## 5. Key Design Decisions (Phase 1)

These decisions were made during the Phase 1 brainstorming session and are binding for implementation.

### 5.1 Syntactic Validity

Skeleton output must be syntactically valid in the original language. AI models reason better about valid code. Idiomatic placeholders are used instead of comment markers:

| Language   | Placeholder                          |
| ---------- | ------------------------------------ |
| Rust       | `todo!()`                            |
| Python     | `pass`                               |
| TypeScript | `throw new Error("not implemented")` |
| JavaScript | `throw new Error("not implemented")` |

### 5.2 Structural Nodes vs. Data Nodes

The engine classifies AST nodes into two categories:

- **Structural Nodes** (functions, methods, closures, impl blocks, classes, modules) — bodies of leaf structural nodes are pruned. Container structural nodes (impl blocks, classes) are recursed into.
- **Everything Else** (structs, enums, types, interfaces, constants, imports) — emitted verbatim, untouched.

The distinction between leaf and container structural nodes is made by the `body_node()` trait method: leaf nodes return `Some(body)`, containers return `None`.

### 5.3 Line Count Metadata

Each pruned body includes a machine-parseable tag showing hidden line count:

```rust
pub fn authenticate(token: &str) -> Result<User, AuthError> {
    todo!() // [skltn: 47 lines hidden]
}
```

Format: `[skltn: N lines hidden]` — structured for AI parsing, human-readable.

### 5.4 Trait-Based Language Backends

Each language implements a `LanguageBackend` trait. The engine delegates all language-specific decisions to the backend:

```rust
pub trait LanguageBackend {
    fn language(&self) -> tree_sitter::Language;
    fn extensions(&self) -> &[&str];
    fn is_structural_node(&self, node: &Node) -> bool;
    fn is_doc_comment(&self, node: &Node, source: &[u8]) -> bool;
    fn body_node<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>>;
    fn placeholder(&self) -> &str;
    fn hidden_line_tag(&self, count: usize) -> String;
    fn format_replacement(&self, indent: &str, line_count: usize, body: &Node, source: &[u8]) -> String;
}
```

JS and TS share logic via a `js_common.rs` module. Adding a new language = implementing this trait.

### 5.5 Byte-Range Replacement Strategy

The engine works on byte ranges, not string concatenation. Replacements are collected as `(start_byte, end_byte, replacement_text)` tuples and applied in reverse order (end-to-start) to preserve offsets. This ensures all original formatting and whitespace outside pruned bodies is preserved exactly.

### 5.6 Configurable Depth

`SkeletonOptions.max_depth: Option<usize>` controls nesting depth. Default is `None` (unlimited). Only leaf structural nodes increment the depth counter. At max depth, subtrees are emitted verbatim.

### 5.7 Imports and Doc Comments

- **All imports preserved** — negligible token cost, high context value for AI reasoning.
- **Doc comments preserved** — `///`, `//!`, `/**` in Rust/TS/JS; docstrings (`"""..."""`) in Python are extracted from function bodies and preserved.
- **Inline comments** inside function bodies are stripped with the body.

### 5.8 CLI Design

```
skltn [OPTIONS] <PATH>

Options:
  --max-depth <N>     Maximum nesting depth (default: unlimited)
  --lang <LANG>       Force language detection override
  --raw               Output without markdown fencing
```

- **Single file:** Skeletonize and print to stdout.
- **Directory:** Recursive traversal via `ignore` crate (respects `.gitignore`), markdown-fenced output with `## File:` headers.
- **TTY-aware:** Markdown fences when outputting to terminal, raw when piping.

### 5.9 Error Handling

- **Unsupported language:** Hard fail with error message.
- **Syntax errors in supported files:** tree-sitter error tolerance — partial parse, emit ERROR nodes verbatim.
- **Files >5,000 lines:** Processed without limits, performance is best-effort beyond the 30ms target.

### 5.10 Python-Specific Handling

- **Indentation:** Engine reads body node's start column from AST for correct placeholder indentation.
- **Docstrings:** `PythonBackend.extract_docstring()` extracts leading triple-quoted strings from function bodies and preserves them in the skeleton.
- **Deeply nested structures:** Indentation stress-tested with dedicated fixtures.
- **Lambdas:** Emitted verbatim (single expressions, cannot be pruned).

### 5.11 Closure and Arrow Function Handling

- **Rust closures:** Block-bodied closures pruned, expression closures emitted verbatim.
- **JS/TS arrow functions:** Block-bodied (`=> { ... }`) pruned, expression-bodied (`=> expr`) emitted verbatim.

### 5.12 Testing Strategy

- **Framework:** `insta` snapshot testing with fixture files per language.
- **Round-trip validation:** Every skeleton is re-parsed by tree-sitter — zero ERROR nodes = valid.
- **Test categories:** Per-language basics, doc comment preservation, line count accuracy, depth limiting, error tolerance, closure/lambda handling, container node recursion, CRLF handling.

---

## 6. Implementation Roadmap

### Phase 1: Skeleton Engine (Spec + Plan Complete)

**Spec:** `docs/superpowers/specs/2026-03-16-phase1-skeleton-engine-design.md`
**Plan:** `docs/superpowers/plans/2026-03-16-phase1-skeleton-engine.md`

21 tasks across 6 chunks:

1. Project scaffolding (workspace, error types, options, trait)
2. Rust backend + engine core
3. Python backend
4. JS/TS backends (shared `js_common.rs`)
5. CLI implementation
6. Edge case fixtures + final validation

### Phase 2: MCP Integration (Spec + Plan Complete)

**Spec:** `docs/superpowers/specs/2026-03-16-phase2-mcp-server-design.md`

Key design decisions:

- `skltn-mcp` binary crate, stateless, repo root as CLI arg, stdio transport via `rmcp`
- 3 tools: `list_repo_structure` (tree listing + byte size + language + `max_depth`), `read_skeleton` (file-only, Budget Guard auto-decides full vs skeleton), `read_full_symbol` (name + `start_line` disambiguation, scope stack for parent context)
- Budget Guard: `tiktoken-rs` (`cl100k_base`) for real token counting, 2k threshold
- Symbol resolution: AST walk covering both structural and data nodes, exact name match, 1-indexed lines
- Hybrid error model: protocol errors for broken requests, content responses for operational feedback
- Path security: canonicalization + prefix check, no info leakage
- `spawn_blocking` for CPU-bound tree-sitter/tiktoken work

### Phase 3: Observability Layer (Spec Complete)

**Spec:** `docs/superpowers/specs/2026-03-16-phase3-observability-layer-design.md`

Key design decisions:
- `skltn-obs` binary crate, standalone (no `skltn-core` dependency), reverse proxy architecture
- Base URL override model: client sets `ANTHROPIC_BASE_URL=http://localhost:PORT`, proxy forwards over HTTPS
- Dual-mode response skimming: non-streaming (buffer + extract) and streaming SSE (background tee + event parsing)
- `UsageRecord` with `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `cost_usd`
- JSONL persistence at `~/.skltn/usage.jsonl` via async background writer task
- Hardcoded pricing in `pricing.rs` with `contains()` matching for model IDs, zero-rate fallback with warning
- WebSocket endpoint at `/ws` for Phase 4 HUD consumption, with session replay on connect
- `axum` for HTTP server + WebSocket, `reqwest` for upstream HTTPS, `time` for timestamps
- Observe and report actuals only — no speculative savings estimation

### Phase 4: Tauri HUD (Not Yet Designed)

- Tauri floating window
- Context usage visualization (% of 200k window)
- "Money Saved" odometer
- WebSocket streaming from observability layer

---

## 7. Success Metrics

| Metric            | Target                                          |
| ----------------- | ----------------------------------------------- |
| Compression Ratio | >75% token reduction per file                   |
| Latency           | <30ms for files under 5,000 lines               |
| Accuracy          | Zero tree-sitter ERROR nodes in skeleton output |
| Language Coverage | Rust, Python, TypeScript, JavaScript (Phase 1)  |

---

## 8. Security & Privacy

- **Local Processing:** All skeletonization runs locally in Rust. No code leaves the machine.
- **Credential Handling:** `.env` or system keychain for API keys.
- **`.gitignore` Respect:** CLI and MCP server honor `.gitignore` via the `ignore` crate.

---

## 9. Project Conventions

- **Each phase is independently testable** — ships a working deliverable before the next begins.
- **All phases are spec'd and planned before any code is written.**
- **Package manager:** Not applicable (pure Rust, Cargo workspace).
- **Spec location:** `docs/superpowers/specs/`
- **Plan location:** `docs/superpowers/plans/`
