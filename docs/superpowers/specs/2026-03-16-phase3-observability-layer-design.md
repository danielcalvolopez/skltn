# Phase 3: Observability Layer — Design Specification

**Project:** skltn (Skeleton)
**Phase:** 3 of 4
**Date:** 2026-03-16
**Status:** Approved

---

## Overview

The Observability Layer is a local reverse proxy that sits between any Anthropic API client and `api.anthropic.com`. It transparently forwards all traffic while extracting token usage and cost data from API responses. Usage records are persisted to a JSONL file and broadcast over WebSocket for real-time consumption by Phase 4's Tauri HUD.

This is Phase 3 of the skltn project. It is **standalone** — no dependency on `skltn-core` (Phase 1) or `skltn-mcp` (Phase 2). The proxy works with any Anthropic API client (Claude Code, Cursor, custom SDK scripts) that supports a `base_url` configuration.

---

## Guiding Principles

1. **Observe, don't speculate.** The proxy reports actual token usage and cost from real API responses. No estimated "savings from skeletonization" — only data that exists in the response payload.
2. **Zero added latency.** The client must receive identical responses with no perceptible delay. Streaming SSE chunks are forwarded immediately; observation happens on cloned data in the background.
3. **Transparent pass-through.** The proxy does not modify requests or responses. It is invisible to both the client and Anthropic beyond the base URL change.

---

## Architecture

### Crate Structure

```
skltn/
├── Cargo.toml                  # Workspace root (add skltn-obs to members)
├── crates/
│   ├── skltn-core/             # Library — Skeleton Engine (Phase 1, unchanged)
│   ├── skltn-cli/              # Binary — CLI wrapper (Phase 1, unchanged)
│   ├── skltn-mcp/              # Binary — MCP server (Phase 2, unchanged)
│   └── skltn-obs/              # Binary — Observability proxy (Phase 3, NEW)
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs         # CLI args (clap), server bootstrap
│           ├── proxy.rs        # Catch-all handler, request forwarding
│           ├── skim.rs         # Response parsing (streaming + non-streaming)
│           ├── pricing.rs      # Hardcoded model pricing lookup
│           ├── tracker.rs      # CostTracker, UsageRecord, JSONL persistence
│           └── ws.rs           # WebSocket endpoint + broadcast
```

### Dependencies

| Crate | Purpose |
|---|---|
| `axum` | HTTP server, routing, WebSocket support |
| `reqwest` | Outbound HTTPS to Anthropic |
| `tokio` | Async runtime |
| `serde`, `serde_json` | JSON serialization |
| `time` | RFC 3339 timestamps (with `serde` and `formatting` features) |
| `clap` | CLI argument parsing |
| `tracing` | Structured logging |

**No dependency on `skltn-core`.** The proxy does not parse ASTs or perform skeletonization. It is a pure network observability tool.

### Phase Dependencies (Updated)

```
Phase 1 (Skeleton Engine) ← standalone
Phase 2 (MCP Server) ← depends on skltn-core
Phase 3 (Observability) ← standalone
Phase 4 (Tauri HUD) ← depends on skltn-obs (WebSocket consumer)
```

### CLI Interface

```
skltn-obs [OPTIONS]

Options:
  --port <PORT>       Local port to listen on (default: 8080)
  --upstream <URL>    Anthropic API base URL (default: https://api.anthropic.com)
  --data-dir <PATH>   Data directory for JSONL persistence (default: ~/.skltn/)
```

**Usage:**
1. Start the proxy: `skltn-obs --port 8080`
2. Configure the AI client: `export ANTHROPIC_BASE_URL=http://localhost:8080`
3. Use the AI client normally — all traffic flows through the proxy

---

## The Reverse Proxy Model

### Why a Reverse Proxy

The proxy uses a **base URL override** approach rather than a forward proxy (HTTPS_PROXY). The client sends plain HTTP to localhost; the proxy makes the upstream HTTPS call to Anthropic.

**Advantages over a forward proxy (MITM):**
- No TLS interception — no certificate generation, no CA trust management
- No corporate security alerts from self-signed certificates
- Every major AI client supports `base_url` configuration natively
- The proxy has cleartext access to JSON payloads on the local leg without cryptographic complexity

### Request Flow

```
AI Client (HTTP) → skltn-obs (localhost:8080) → Anthropic API (HTTPS)
                 ← response with usage data  ←
                 ↓
            CostTracker → JSONL file
                 ↓
            WebSocket → Phase 4 HUD
```

---

## Proxy Handler (`proxy.rs`)

### Routing

```rust
let app = Router::new()
    .route("/ws", get(ws_handler))
    .fallback(proxy_handler)
    .with_state(state);
```

The `/ws` route is explicitly registered for WebSocket connections. Everything else hits the catch-all `proxy_handler`, which forwards transparently to Anthropic. This means `/v1/messages`, `/v1/models`, `/v1/complete`, and any future Anthropic endpoints work without proxy changes.

### Request Forwarding

1. Reconstruct the upstream URL: `{upstream_base}{original_path}?{original_query}`
2. Copy all headers from the incoming request (`Authorization`, `Content-Type`, `anthropic-version`, etc.)
3. Copy the request body
4. Send via the shared `reqwest::Client`
5. Copy response status and headers back to the client
6. Route the response body through the skimmer based on `Content-Type`

### Model Extraction

Before forwarding `POST /v1/messages` requests, the proxy deserializes just enough of the request body to extract the `model` field. This is needed for pricing calculation. The original bytes are forwarded unchanged. For non-message endpoints, no body parsing occurs.

If request body parsing fails (malformed JSON, missing `model` field), the request is still forwarded to Anthropic (transparent pass-through is never broken). No `UsageRecord` is generated for that request, and a `tracing::warn!` is emitted. Anthropic will return its own error response for truly malformed requests.

### What the Proxy Does NOT Do

- No request modification — bodies and headers pass through untouched
- No authentication — the client's API key flows through as-is
- No caching — every request hits Anthropic
- No rate limiting — that's Anthropic's responsibility

### `reqwest::Client` Configuration

The client is initialized once in `main.rs` and shared via `axum::State`:

- `redirect(Policy::none())` — don't follow redirects, pass them through to the client
- `tcp_nodelay(true)` — minimize SSE chunk latency
- `timeout(Duration::from_secs(300))` — 5-minute timeout for long AI responses
- Connection pooling enabled by default

---

## Response Skimming (`skim.rs`)

The skimmer extracts usage data from Anthropic API responses. Two distinct code paths based on the **response `Content-Type` header**, which is the authoritative signal for determining the response format. The `stream` field from the request body is not used for this decision — the response header reflects what actually happened.

### Non-Streaming Path

When the response `Content-Type` is `application/json`:

1. Buffer the full response body via `response.bytes().await`
2. Parse as JSON, extract the `usage` object from the top level
3. Build `UsageRecord`, send to `CostTracker`
4. Return the buffered bytes to the client unchanged

The response body is a single JSON object with `usage` at the top level:

```json
{
  "id": "msg_...",
  "content": [...],
  "usage": {
    "input_tokens": 2500,
    "output_tokens": 800,
    "cache_creation_input_tokens": 0,
    "cache_read_input_tokens": 1200
  }
}
```

### Streaming Path

When the response `Content-Type` is `text/event-stream`:

The response is a series of SSE events. Usage data is split across two events:

- **`message_start`** — contains `input_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`
- **`message_delta`** — contains `output_tokens` (arrives near end of stream)

```
event: message_start
data: {"type":"message_start","message":{"usage":{"input_tokens":2500,"cache_creation_input_tokens":0,"cache_read_input_tokens":1200}}}

... content_block_start, content_block_delta events ...

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":800}}

event: message_stop
data: {"type":"message_stop"}
```

**The "Background Tee" approach:**

1. Create a `tokio::sync::mpsc` channel for forwarding chunks to the client
2. Spawn a background task that reads chunks from the upstream `reqwest` response stream
3. For each chunk, the background task:
   - Sends a **clone** of the `Bytes` to the channel (for immediate forwarding to the client)
   - Appends to an internal `String` buffer for SSE event parsing
   - Scans the buffer for a complete SSE event boundary (`\n\n`). When found, everything up to and including the boundary is parsed as a complete SSE event, and that portion is removed from the buffer. Any trailing bytes after the boundary remain in the buffer as the start of the next event. This handles events split across chunk boundaries correctly — partial data simply accumulates until the next `\n\n` arrives.
   - Extracts usage fields from `message_start` and `message_delta` events into a `partial_record` held in local scope
4. The axum response body wraps the channel receiver via `Body::from_stream()` — chunks flow to the client immediately as they arrive
5. When the stream ends (or `message_stop` is received), the background task merges the partial fields into a final `UsageRecord` and sends it to the `CostTracker`

**Mid-stream error handling:**

If the upstream connection drops or errors mid-stream (before `message_stop` is received):
- The forwarding channel is closed, so the client sees the stream end (matching what would happen without the proxy)
- If a `message_start` was received but no `message_delta`, the partial record is **discarded** — no `UsageRecord` is generated. Incomplete data would skew cost tracking, and the API call likely failed anyway.
- A `tracing::warn!` is emitted noting the incomplete stream

**Key properties:**
- Zero added latency — chunks forwarded as received, parsing happens on cloned data
- Memory-safe — parsed events are removed from the buffer, only unparsed trailing bytes accumulate (typically a few hundred bytes at most)
- Structurally robust — uses SSE event boundary parsing (`\n\n`), not substring scanning on raw bytes

---

## Data Model & Persistence (`tracker.rs`)

### `UsageRecord`

The atomic unit of observability data. One record per API response.

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UsageRecord {
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub model: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cache_creation_input_tokens: usize,
    pub cache_read_input_tokens: usize,
    pub cost_usd: f64,
}
```

### `CostTracker`

Lives in `axum::State`, wrapped in `Arc<Mutex<CostTracker>>`. The `Mutex` contention is negligible — locked for microseconds to push a record and send on channels.

```rust
pub struct CostTracker {
    records: Vec<UsageRecord>,
    file_writer: mpsc::Sender<UsageRecord>,
    broadcast: broadcast::Sender<UsageRecord>,
}
```

- `records` — in-memory session buffer, enables WebSocket replay on connect
- `file_writer` — sends records to a dedicated background writer task
- `broadcast` — fans out records to all connected WebSocket clients

### JSONL Persistence

- **File path:** `{data_dir}/usage.jsonl` (default: `~/.skltn/usage.jsonl`)
- **Writer task:** A dedicated `tokio::spawn` task receives records via `mpsc` and appends to the file using `OpenOptions::new().append(true).create(true)`
- **Directory creation:** `~/.skltn/` created on startup if missing
- **Atomic appends:** One line per record, each line is a complete JSON object. Even if the proxy crashes mid-write, previous records remain intact.
- **Non-blocking:** The HTTP response path never waits on disk I/O. Records are sent to the writer via the async `mpsc` channel.
- **Graceful shutdown:** On `SIGINT`/`SIGTERM`, the writer task drains any remaining records from the `mpsc` channel before exiting. This prevents the last few records from being lost during clean shutdown.

---

## Pricing (`pricing.rs`)

### Model Rates

Hardcoded pricing lookup. A single, obvious module to update when Anthropic changes prices.

```rust
pub struct ModelRates {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_write_per_mtok: f64,
    pub cache_read_per_mtok: f64,
}

/// Rates as of 2026-03-16. Verify against https://docs.anthropic.com/en/docs/about-claude/models
/// before implementation — prices may have changed.
pub fn get_rates(model: &str) -> ModelRates {
    match model {
        m if m.contains("claude-opus-4") => ModelRates {
            input_per_mtok: 15.00, output_per_mtok: 75.00,
            cache_write_per_mtok: 18.75, cache_read_per_mtok: 1.50,
        },
        m if m.contains("claude-sonnet-4") => ModelRates {
            input_per_mtok: 3.00, output_per_mtok: 15.00,
            cache_write_per_mtok: 3.75, cache_read_per_mtok: 0.30,
        },
        m if m.contains("claude-haiku-4") => ModelRates {
            input_per_mtok: 0.80, output_per_mtok: 4.00,
            cache_write_per_mtok: 1.00, cache_read_per_mtok: 0.08,
        },
        m if m.contains("claude-3-7-sonnet") => ModelRates {
            input_per_mtok: 3.00, output_per_mtok: 15.00,
            cache_write_per_mtok: 3.75, cache_read_per_mtok: 0.30,
        },
        m if m.contains("claude-3-5-sonnet") => ModelRates {
            input_per_mtok: 3.00, output_per_mtok: 15.00,
            cache_write_per_mtok: 3.75, cache_read_per_mtok: 0.30,
        },
        m if m.contains("claude-3-5-haiku") => ModelRates {
            input_per_mtok: 0.80, output_per_mtok: 4.00,
            cache_write_per_mtok: 1.00, cache_read_per_mtok: 0.08,
        },
        _ => {
            tracing::warn!("Unknown model '{}', cost tracking will show $0.00", model);
            ModelRates::zero()
        }
    }
}
```

- **`contains()` matching** handles date-stamped model IDs (e.g., `claude-sonnet-4-6-20260301`) without needing exact version strings
- **Unknown models** return zero rates with a `tracing::warn!` so the user sees incomplete tracking in their terminal
- **No config file** — hardcoded is sufficient for a developer tool where prices change ~twice a year

### Cost Calculation

```rust
pub fn calculate_cost(record: &UsageRecord, rates: &ModelRates) -> f64 {
    (record.input_tokens as f64 * rates.input_per_mtok
        + record.output_tokens as f64 * rates.output_per_mtok
        + record.cache_creation_input_tokens as f64 * rates.cache_write_per_mtok
        + record.cache_read_input_tokens as f64 * rates.cache_read_per_mtok)
        / 1_000_000.0
}
```

Cost is calculated once when the `UsageRecord` is created, stored in `cost_usd`, then persisted and broadcast.

---

## WebSocket Interface (`ws.rs`)

### Endpoint

`GET /ws` — upgrades to WebSocket connection.

### Protocol

- **Server → Client only** — no client-to-server messages expected
- Each message is a JSON-serialized `UsageRecord`

### Connection Lifecycle

1. Client connects to `ws://localhost:PORT/ws`
2. **Replay:** Server sends all `UsageRecord`s from the current in-memory session buffer (`CostTracker.records`). This lets a HUD that connects mid-session catch up instantly.
3. **Live:** Server forwards each new record from the `broadcast` channel as it arrives
4. On disconnect, the subscription is dropped. No cleanup needed — `broadcast` handles this.

**Critical invariant — replay-to-live handoff:** The `records.clone()` and `broadcast.subscribe()` calls must happen inside the same `Mutex` lock acquisition. This guarantees no gap between the replayed history and the live stream. Because `CostTracker::record()` also acquires the lock to both push to `records` and send on `broadcast`, a record is either in the cloned vector (if it was added before the lock) or will be received via the broadcast subscription (if it's added after). Moving the `subscribe()` call outside the lock would introduce a race where records could be missed.

### Implementation

```rust
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(tracker): State<Arc<Mutex<CostTracker>>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws(socket, tracker))
}

async fn handle_ws(mut socket: WebSocket, tracker: Arc<Mutex<CostTracker>>) {
    // 1. Replay existing records
    let (records, mut rx) = {
        let t = tracker.lock().unwrap();
        (t.records.clone(), t.broadcast.subscribe())
    };
    for record in records {
        let msg = serde_json::to_string(&record).unwrap();
        if socket.send(Message::Text(msg)).await.is_err() { return; }
    }

    // 2. Stream live records
    while let Ok(record) = rx.recv().await {
        let msg = serde_json::to_string(&record).unwrap();
        if socket.send(Message::Text(msg)).await.is_err() { return; }
    }
}
```

### Broadcast Channel

- **Channel size:** `broadcast::channel(64)` — if a slow consumer falls 64 messages behind, it receives `RecvError::Lagged` and the connection is dropped. This prevents a stuck HUD from causing unbounded memory growth.
- **Why no JSONL replay:** The JSONL file is the persistent ledger across proxy restarts. The in-memory buffer covers the current session. If the HUD needs historical data across sessions, it reads the JSONL file directly on startup — that's a Phase 4 concern.

---

## Testing Strategy

### Test Categories

| Category | What It Validates |
|---|---|
| Non-streaming proxy | Request forwarded correctly, response returned unchanged, `UsageRecord` extracted |
| Streaming proxy (SSE) | Chunks forwarded in real-time, `message_start` + `message_delta` merged into single `UsageRecord` |
| Pricing calculation | `calculate_cost()` produces correct USD for known models, zero for unknown with warning |
| JSONL persistence | Records appended correctly, file created on first write, survives multiple records |
| WebSocket replay | Connecting mid-session receives all prior records, then live records |
| WebSocket broadcast | Multiple connected clients all receive the same records |
| Model extraction | `model` and `stream` fields correctly extracted from request body |
| Unknown model handling | Zero-rate fallback, `tracing::warn!` emitted |
| Pass-through for non-message endpoints | `/v1/models`, unknown paths forwarded without body parsing |
| Error resilience | Upstream timeout, upstream 500, malformed response body — proxy doesn't crash, returns error to client |

### Mock Upstream Server

Tests use a **mock `axum` server** in the test harness that mimics Anthropic's response format. This enables:
- Controlled `usage` blocks with known values
- Simulated streaming SSE responses with precise chunk boundaries
- Edge case testing (malformed JSON, missing `usage` field, unexpected event types)
- Offline execution — no API key needed, fast test runs

### Testing `skim.rs` Specifically

The SSE parsing logic is the most complex component. Dedicated tests for:
- Events split across multiple chunks (partial event in one chunk, remainder in next)
- `message_start` without a subsequent `message_delta` (incomplete stream)
- Unexpected event types between `message_start` and `message_delta`
- Large content payloads between the two usage-bearing events

---

## Success Criteria (Phase 3)

| Metric | Target |
|---|---|
| Proxy transparency | Client receives identical status, headers, and body as a direct Anthropic call |
| SSE latency | Zero added latency — chunks forwarded as received |
| Usage extraction | 100% of `usage` data captured from both streaming and non-streaming responses |
| Persistence | All `UsageRecord`s written to JSONL, no data loss on clean shutdown |
| WebSocket delivery | HUD receives records within milliseconds of response completion |
| Cost accuracy | `cost_usd` matches manual calculation from token counts and published rates |

---

## Out of Scope (Phase 3)

- Tauri HUD / visualization (Phase 4)
- Historical data analysis or aggregation queries over JSONL
- Speculative "savings from skeletonization" calculation
- HTTPS on the local proxy leg (localhost only, plain HTTP)
- Request modification or response caching
- Multi-upstream support (single Anthropic endpoint only)
- Authentication or access control on the proxy itself
- HTTP/2 or HTTP/3 on the local leg
- Configuration file for pricing (hardcoded is sufficient)
