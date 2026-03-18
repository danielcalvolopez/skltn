---
name: Key technical decisions for skltn
description: Binding design decisions made during brainstorming that all future sessions must follow
type: project
---

## Phase 1 Decisions (Binding)

1. **Syntactic validity over comment markers** — Use `todo!()`, `pass`, `throw new Error()` instead of `// ... implementation hidden`
2. **Trait-based language backends** (Approach B) — not config tables, not tree-sitter queries
3. **JS/TS shared logic** via `js_common.rs` — avoids 80% code duplication
4. **No `is_data_node()` on trait** — binary logic: structural → prune, everything else → verbatim
5. **Container vs leaf distinction via `body_node()`** — containers return None, leaves return Some(body)
6. **`format_replacement()` on trait** — language-specific (braces for Rust/JS/TS, indentation for Python)
7. **`is_doc_comment()` takes source bytes** — needed to distinguish `///` from `//`
8. **Preserve all imports** — negligible token cost, high AI context value
9. **`[skltn: N lines hidden]` tag format** — machine-parseable, human-readable
10. **Reverse byte-range replacement** — apply from end to start to preserve offsets
11. **`ignore` crate for traversal** — respects .gitignore automatically
12. **Markdown headers outside fences** — `## File:` headers are stronger AI landmarks than code comments
13. **`is-terminal` for TTY detection** — auto-toggle markdown fencing
14. **Python docstring preservation** — `extract_docstring()` pulls leading triple-quoted strings from bodies
15. **Expression closures/arrows emitted verbatim** — only block-bodied are pruned
16. **`#[cfg(test)]` modules** — treated as containers, test function bodies pruned individually
17. **Solidity deferred** — replaced with JavaScript in Phase 1, Solidity in future phase

## Phase 2 Decisions (Binding)

1. **Three tools only** — `list_repo_structure`, `read_skeleton`, `read_full_symbol`. No directory skeleton tool.
2. **`read_full_symbol` hybrid lookup** — name-based primary, `start_line` for disambiguation
3. **Budget Guard uses `tiktoken-rs`** (`cl100k_base`) — real token counting, not byte heuristics
4. **2k token threshold** — constant, not configurable
5. **`list_repo_structure` returns metadata** — file paths + byte size + language, with `max_depth` param
6. **`read_skeleton` is file-only** — AI skeletons files individually, no directory batching
7. **Stateless server** — no cached state, process lifetime = session, repo root as CLI arg
8. **Hybrid error model** — protocol errors for broken requests, content responses for operational feedback
9. **Data node identification in `resolve.rs`** — hardcoded node kind strings, NOT a trait method on `LanguageBackend`
10. **`McpError` is MCP-local** — separate from `skltn-core::SkltnError`, wraps it via `Core()` variant
11. **Path security via canonicalization** — `canonicalize()` + `starts_with()` prefix check, no path info leakage
12. **1-indexed lines at MCP boundary** — convert from tree-sitter 0-indexed in `resolve.rs`
13. **Scope stack for parent context** — container names pushed/popped during AST walk for disambiguation
14. **Doc comments/decorators via sibling look-back** — extraction range starts at first preceding doc comment or decorator
15. **`spawn_blocking` for CPU-bound work** — tree-sitter and tiktoken wrapped for async transport
16. **Empty directories pruned** — `list_repo_structure` omits directories with no supported files recursively
17. **File size limit (10 MB)** — `read_skeleton` and `read_full_symbol` check file size before reading. Files > 10 MB return content response `"File too large: {path} ({size} bytes, limit is 10 MB)"`. Prevents OOM from oversized files.

## Phase 3 Decisions (Binding)

1. **Reverse proxy / base URL override** — not forward proxy (MITM). Client sets `ANTHROPIC_BASE_URL=http://localhost:PORT`.
2. **No speculative savings calculation** — observe and report actuals only. PRD "savings" requirement narrowed to actual cost tracking.
3. **`axum` for HTTP server** — routing, WebSocket, Tower middleware
4. **`reqwest` for upstream HTTPS** — connection pooling, `tcp_nodelay(true)`, 5-min timeout
5. **Dual-mode response skimming** — non-streaming (buffer + extract) and streaming SSE (background tee + event parsing)
6. **Response `Content-Type` is authoritative** for stream mode detection — not the request body `stream` field
7. **SSE accumulate-then-drain buffer** — accumulates bytes until `\n\n` boundary, parses complete event, retains trailing partial data
8. **Merge `message_start` + `message_delta`** into single `UsageRecord` — input tokens from start, output tokens from delta
9. **Mid-stream errors discard partial records** — incomplete data would skew cost tracking
10. **`CostTracker` with JSONL persistence** — `~/.skltn/usage.jsonl`, async background writer via `mpsc` channel
11. **Hardcoded pricing in `pricing.rs`** — `contains()` matching for model IDs, zero-rate fallback with `tracing::warn!`
12. **`time` crate for timestamps** — RFC 3339 serialization, replaces `chrono`
13. **Raw `UsageRecord` broadcast over WebSocket** — `/ws` endpoint, `broadcast::channel(64)`, session replay on connect
14. **Replay-to-live lock invariant** — `records.clone()` and `broadcast.subscribe()` must be in same Mutex lock
15. **`skltn-obs` is standalone** — no dependency on `skltn-core`, pure network observability
16. **Graceful shutdown drains JSONL writer** — `mpsc` channel drained on `SIGINT`/`SIGTERM`
17. **Model extraction failure is non-fatal** — request forwarded, no `UsageRecord` generated, warning logged

### Phase 3 Security Hardening (from review)

18a. **`--allow-external` gate for non-loopback bind** — `127.0.0.1` bind address by default. Non-loopback bind **refuses to start** unless `--allow-external` flag is also passed. When used, prints a prominent stderr banner about API key exposure (not just a log line).
18b. **Request body size limit** — 200 MB via `axum::extract::DefaultBodyLimit` to prevent OOM from oversized requests.
18c. **No header logging** — Request headers (containing `x-api-key`) must never be logged, even at DEBUG/TRACE. Only log method, path, and response status.
18d. **Restrictive file permissions** — `~/.skltn/` directory created with `0o700`, JSONL file with `0o600` (Unix).
18e. **`--upstream` HTTPS enforcement** — Must use HTTPS unless host is loopback. Non-Anthropic upstreams emit `tracing::warn!`. TLS cert verification must remain enabled (no `--insecure` flag).
18f. **Model name validation** — Validated against `^[a-zA-Z0-9._-]+$`. Invalid names replaced with `"unknown"` + warning. Prevents attacker-controlled strings in JSONL/WebSocket.
18g. **SSE buffer size limit** — 10 MB max. Discards buffer and stops parsing (continues forwarding) if exceeded. Prevents OOM from malformed/malicious upstream.
18h. **WebSocket Origin validation** — `/ws` rejects connections with non-localhost `Origin` headers. Prevents cross-site WebSocket hijacking.
18i. **`regex` + `url` crates added** — for model name validation and upstream URL parsing respectively.

### Phase 3 Plan Refinements (from implementation planning)

18. **`lib.rs` + `main.rs` split** — library crate for integration test imports, binary for CLI entry point
19. **`CostTracker` wraps `Arc<Mutex<CostTrackerInner>>`** — internal synchronization (Clone-able), simplification over spec's `Arc<Mutex<CostTracker>>` at the State level. Uses `tokio::sync::Mutex` (async-aware).
20. **`CostTracker::shutdown()` with JoinHandle** — deterministic JSONL drain via stored JoinHandle, not drop + sleep
21. **`calculate_cost` takes individual token counts** — not `&UsageRecord` as in spec. Avoids chicken-and-egg (cost needed before record is constructed).
22. **`ws_handler` takes `State<AppState>`** — not `State<CostTracker>`. Matches axum router state type. Accesses `state.tracker`.
23. **`snapshot_and_subscribe()` method** — encapsulates the replay-to-live lock invariant from spec decision #14

## Phase 4 Decisions (Binding)

1. **Web dashboard, not Tauri** — PRD originally specified Tauri desktop app. Changed to web dashboard served from `skltn-obs` because: the Rust backend logic is trivial (just a WebSocket consumer), Tauri adds build complexity for no benefit, and the proxy already runs an axum server.
2. **No separate `skltn-hud` crate** — Phase 4 modifies `skltn-obs` (adds `dashboard.rs` + static assets). No new Cargo workspace member.
3. **Vite + React + TypeScript** — frontend framework. Not Next.js (SSR/routing unnecessary for single-page dashboard). Not vanilla JS (React DX preferred).
4. **`rust-embed` for production assets** — bakes `dashboard/dist/` into binary at compile time. Single binary distribution.
5. **`mime_guess` for Content-Type** — serves static files with correct MIME types.
6. **`nest_service("/dashboard")` routing** — dashboard nested under `/dashboard` to avoid shadowing Anthropic API paths. `GET /` redirects to `/dashboard`.
7. **Vite `base: '/dashboard/'`** — ensures asset paths match the nested route.
8. **Vite proxy for dev** — `server.proxy` forwards `/ws` to Axum. No CORS needed. No conditional Rust logic for dev vs prod.
9. **`justfile` for build orchestration** — `build-ui` (pnpm), `build` (ui + cargo), `dev` (cargo only).
10. **Matrix Green on obsidian aesthetic** — TUI/brutalist HUD look. `#00ff88` accent, `#0a0a0a` background, JetBrains Mono, sharp 1px borders, no rounded corners.
11. **ECharts via `echarts-for-react`** — charting library. `step: 'end'` for sharp angles, `shadowBlur` for CRT glow, no area fill. Time-based X axis.
12. **Approach B state management** — three custom hooks: `useObsWebSocket` (connection + records), `useSessionMetrics` (derived totals), `useChartData` (ECharts options). Zero external state libraries.
13. **Exponential backoff reconnection** — 1s initial, 2x multiplier, 30s cap. Backoff stored in `useRef` (not state). Connection status exposed as `'connecting' | 'open' | 'closed'`.
14. **Cache savings = real metric** — `cache_read_tokens * (input_rate - cache_read_rate) / 1_000_000`. Not speculative. Based on actual Anthropic pricing delta.
15. **7-column request table** — timestamp, model, input, output, cache write, cache read, cost. Newest first, sticky header, zebra stripe.
16. **4 headline metrics** — session cost (USD), cache savings (USD), request count, total tokens.
17. **Sidebar: cache hit ratio ring + per-model breakdown** — SVG ring for cache %, bar chart for model costs.
18. **No scanline CSS overlay** — readability > aesthetics for a tool you stare at while coding.
19. **No additional JS dependencies** — no Zustand/Redux, no CSS framework, no charting beyond ECharts. Hand-written CSS.
20. **Pure computation functions extracted** — `calculateSessionTotals` and `calculateCacheSavings` as pure functions, testable if needed later.
21. **Manual testing for initial build** — no unit tests for dashboard components. Pure functions extracted for future testability.

### Phase 4 Plan Refinements (from implementation planning)

22. **`Redirect::temporary` for 302** — not `Redirect::to` which produces 303 in axum 0.8+. Spec requires 302.
23. **`tsconfig.json` + `tsconfig.app.json` split** — `tsc -b` requires project references. `tsconfig.json` has `references`, `tsconfig.app.json` has `"composite": true`. Standard Vite scaffold pattern.
24. **`index.html` script src without base prefix** — uses `/src/main.tsx`, not `/dashboard/src/main.tsx`. Vite's `base: '/dashboard/'` config handles prefixing at build time; including it in `index.html` would double-prefix.
25. **Phase 3 prerequisite explicitly stated** — Phase 4 assumes `skltn-obs` crate exists with `lib.rs` containing Phase 3 module declarations. Binary imports from library crate via `use skltn_obs::{proxy, ws}`.
26. **Security headers on all dashboard responses** — CSP (`default-src 'self'; connect-src 'self' ws://localhost:* ws://127.0.0.1:*; style-src 'self' 'unsafe-inline'; font-src 'self'`), `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`.
27. **Cache-Control headers** — `no-cache` for `index.html`, `public, max-age=31536000, immutable` for hashed assets.

## Cross-Phase Amendment: Prompt Caching Economics (2026-03-17)

1. **Cache-aware Budget Guard** — Skeletonizing cached files is 2.5x more expensive than serving full. Budget Guard redesigned with `CacheHint` enum + `SessionTracker`.
2. **CacheHint enum with 4 variants** — `Unknown` (cold start), `RecentlyServed` (Phase 2 heuristic), `CacheConfirmed` (Phase 3 actual), `CacheExpired` (Phase 3 stale). Phase 3 variants defined but not wired in Phase 2.
3. **SessionTracker: `HashMap<PathBuf, Instant>`** — In-memory tracker of files served full. Process lifetime = session. No eviction needed.
4. **First read still skeletonizes** — Cache-awareness only kicks in on subsequent reads of files previously served full.
5. **`read_full_symbol` does NOT update tracker** — Fragments don't match full-file cache entries.
6. **Phase 3 integration deferred** — `UsageRecord.cache_read_input_tokens` already exists. Integration mechanism (JSONL read, HTTP endpoint, or IPC) to be designed when Phase 3 is underway.
7. **~80% accuracy with Phase 2 alone, ~98% with Phase 3 integration** — Heuristic for cold-start, actuals for steady-state.
8. **Metadata suffix `(cache-aware)`** — Response header indicates when a file was served full due to caching economics rather than being under threshold. Only shown when `original_tokens > TOKEN_THRESHOLD` AND hint was `RecentlyServed`/`CacheConfirmed` — small files never get the tag since they'd be full anyway.
9. **`TOKEN_THRESHOLD` made `pub const`** — `read_skeleton.rs` needs to check `original_tokens > TOKEN_THRESHOLD` for the `(cache-aware)` tag decision.
10. **Skeletonized files NOT recorded in tracker** — Only files served full are recorded. Skeleton token sequence differs from full file, so wouldn't benefit from provider's cache.
11. **Small files ARE recorded in tracker** — Even under-threshold files are genuinely cached by the provider. If the file grows between reads (user adds code), the hint correctly prevents skeletonization of the now-larger file.

**Why:** Anthropic's prompt caching (90% discount on repeated input) means "compressing" a file by skeletonizing it can be more expensive than caching the full file. The Budget Guard must account for caching economics, not just token count.

**How to apply:** Phase 2 implementation plan at `docs/superpowers/plans/2026-03-17-cache-aware-budget-guard.md` (5 tasks, 4 chunks). Phase 3 spec has a future integration note. Do not revisit these decisions without explicit user request.
