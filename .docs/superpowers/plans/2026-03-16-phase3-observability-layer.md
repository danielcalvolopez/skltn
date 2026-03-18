# Phase 3: Observability Layer Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a local reverse proxy (`skltn-obs`) that sits between any Anthropic API client and `api.anthropic.com`, transparently forwarding all traffic while extracting token usage and cost data for real-time WebSocket broadcast and JSONL persistence.

**Architecture:** New `skltn-obs` binary crate in the existing Cargo workspace. Axum HTTP server with catch-all proxy handler forwarding requests to Anthropic via reqwest. Dual-mode response skimming (non-streaming JSON buffer + streaming SSE background tee). CostTracker persists UsageRecords to JSONL via async mpsc writer and broadcasts to WebSocket subscribers. Standalone — no dependency on `skltn-core`.

**Tech Stack:** Rust (latest stable), axum (HTTP + WebSocket), reqwest (upstream HTTPS), tokio, serde/serde_json, time (RFC 3339), clap, tracing/tracing-subscriber

**Spec:** `docs/superpowers/specs/2026-03-16-phase3-observability-layer-design.md`

---

## File Structure

```
crates/skltn-obs/
├── Cargo.toml
├── src/
│   ├── lib.rs          # Library root — pub mod declarations (for integration test imports)
│   ├── main.rs         # CLI args (clap), server bootstrap, graceful shutdown
│   ├── proxy.rs        # Catch-all handler, request forwarding, model extraction
│   ├── skim.rs         # Response parsing (streaming + non-streaming), UsageRecord construction
│   ├── pricing.rs      # Hardcoded model pricing lookup, cost calculation
│   ├── tracker.rs      # CostTracker, UsageRecord struct, JSONL background writer
│   └── ws.rs           # WebSocket upgrade handler, replay + live broadcast
└── tests/
    ├── pricing_test.rs           # Unit tests for pricing lookup and cost calculation
    ├── tracker_test.rs           # CostTracker record/retrieve, JSONL file writing
    ├── proxy_nonstreaming_test.rs # End-to-end non-streaming proxy with mock upstream
    ├── proxy_streaming_test.rs    # End-to-end streaming SSE proxy with mock upstream
    └── ws_test.rs                 # WebSocket replay and live broadcast
```

**Responsibilities per file:**

| File | Responsibility |
|---|---|
| `main.rs` | Parse CLI args (port, upstream URL, data dir), build AppState, init reqwest client, start axum server with graceful shutdown |
| `proxy.rs` | `proxy_handler()` — reconstruct upstream URL, copy headers/body, forward via reqwest, route response through skimmer based on Content-Type |
| `skim.rs` | `skim_nonstreaming()` — buffer JSON response, extract usage. `skim_streaming()` — background tee with SSE event parsing, merge message_start + message_delta |
| `pricing.rs` | `ModelRates` struct, `get_rates()` with contains-matching, `calculate_cost()`, `ModelRates::zero()` |
| `tracker.rs` | `UsageRecord` struct with serde + time, `CostTracker` (records vec + broadcast + file_writer mpsc), `spawn_jsonl_writer()` background task |
| `ws.rs` | `ws_handler()` upgrade, `handle_ws()` replay-then-live loop, subscribe inside Mutex lock for gap-free handoff |

---

## Chunk 1: Crate Scaffolding, Pricing, and UsageRecord

### Task 1: Add skltn-obs Crate to Workspace

**Files:**
- Modify: `Cargo.toml` (workspace root — add `"crates/skltn-obs"` to members)
- Create: `crates/skltn-obs/Cargo.toml`
- Create: `crates/skltn-obs/src/lib.rs`
- Create: `crates/skltn-obs/src/main.rs`

- [ ] **Step 1: Add skltn-obs to workspace members**

In the workspace root `Cargo.toml`, add `"crates/skltn-obs"` to the `members` list:

```toml
[workspace]
resolver = "2"
members = ["crates/skltn-core", "crates/skltn-cli", "crates/skltn-mcp", "crates/skltn-obs"]
```

- [ ] **Step 2: Create skltn-obs Cargo.toml**

`crates/skltn-obs/Cargo.toml`:
```toml
[package]
name = "skltn-obs"
version = "0.1.0"
edition = "2021"

[lib]
name = "skltn_obs"
path = "src/lib.rs"

[[bin]]
name = "skltn-obs"
path = "src/main.rs"

[dependencies]
axum = { version = "0.8", features = ["ws"] }
reqwest = { version = "0.12", features = ["stream"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
time = { version = "0.3", features = ["serde", "formatting", "parsing", "macros"] }
clap = { version = "4", features = ["derive"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
bytes = "1"
futures = "0.3"
http = "1"
http-body-util = "0.1"
regex = "1"
url = "2"

[dev-dependencies]
tokio-tungstenite = "0.26"
tempfile = "3"
```

- [ ] **Step 3: Create lib.rs and stub main.rs**

`crates/skltn-obs/src/lib.rs` (library root — integration tests import from here):
```rust
// Module declarations are added as each module is implemented.
```

`crates/skltn-obs/src/main.rs`:
```rust
fn main() {
    println!("skltn-obs - not yet implemented");
}
```

- [ ] **Step 4: Verify workspace compiles**

Run: `cargo build -p skltn-obs`
Expected: Successful compilation.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/skltn-obs/
git commit -m "chore: add skltn-obs crate to workspace"
```

---

### Task 2: Implement Pricing Module

**Files:**
- Create: `crates/skltn-obs/src/pricing.rs`
- Create: `crates/skltn-obs/tests/pricing_test.rs`
- Modify: `crates/skltn-obs/src/lib.rs` (add module declaration)

- [ ] **Step 1: Write the failing tests**

`crates/skltn-obs/tests/pricing_test.rs`:
```rust
use skltn_obs::pricing::{get_rates, calculate_cost, ModelRates};

#[test]
fn test_opus_rates() {
    let rates = get_rates("claude-opus-4-6-20260301");
    assert_eq!(rates.input_per_mtok, 15.00);
    assert_eq!(rates.output_per_mtok, 75.00);
    assert_eq!(rates.cache_write_per_mtok, 18.75);
    assert_eq!(rates.cache_read_per_mtok, 1.50);
}

#[test]
fn test_sonnet_rates() {
    let rates = get_rates("claude-sonnet-4-6-20260301");
    assert_eq!(rates.input_per_mtok, 3.00);
    assert_eq!(rates.output_per_mtok, 15.00);
    assert_eq!(rates.cache_write_per_mtok, 3.75);
    assert_eq!(rates.cache_read_per_mtok, 0.30);
}

#[test]
fn test_haiku_rates() {
    let rates = get_rates("claude-haiku-4-5-20251001");
    assert_eq!(rates.input_per_mtok, 0.80);
    assert_eq!(rates.output_per_mtok, 4.00);
    assert_eq!(rates.cache_write_per_mtok, 1.00);
    assert_eq!(rates.cache_read_per_mtok, 0.08);
}

#[test]
fn test_legacy_sonnet_35_rates() {
    let rates = get_rates("claude-3-5-sonnet-20241022");
    assert_eq!(rates.input_per_mtok, 3.00);
    assert_eq!(rates.output_per_mtok, 15.00);
}

#[test]
fn test_legacy_sonnet_37_rates() {
    let rates = get_rates("claude-3-7-sonnet-20250219");
    assert_eq!(rates.input_per_mtok, 3.00);
    assert_eq!(rates.output_per_mtok, 15.00);
}

#[test]
fn test_legacy_haiku_35_rates() {
    let rates = get_rates("claude-3-5-haiku-20241022");
    assert_eq!(rates.input_per_mtok, 0.80);
    assert_eq!(rates.output_per_mtok, 4.00);
}

#[test]
fn test_unknown_model_returns_zero() {
    let rates = get_rates("gpt-4o-unknown");
    assert_eq!(rates.input_per_mtok, 0.0);
    assert_eq!(rates.output_per_mtok, 0.0);
    assert_eq!(rates.cache_write_per_mtok, 0.0);
    assert_eq!(rates.cache_read_per_mtok, 0.0);
}

#[test]
fn test_calculate_cost_opus() {
    let rates = get_rates("claude-opus-4-6-20260301");
    let cost = calculate_cost(2500, 800, 0, 1200, &rates);
    // (2500 * 15.00 + 800 * 75.00 + 0 * 18.75 + 1200 * 1.50) / 1_000_000
    // = (37500 + 60000 + 0 + 1800) / 1_000_000
    // = 99300 / 1_000_000
    // = 0.0993
    let expected = 0.0993;
    assert!((cost - expected).abs() < 1e-10, "Expected {expected}, got {cost}");
}

#[test]
fn test_calculate_cost_zero_tokens() {
    let rates = get_rates("claude-sonnet-4-6-20260301");
    let cost = calculate_cost(0, 0, 0, 0, &rates);
    assert_eq!(cost, 0.0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p skltn-obs --test pricing_test`
Expected: FAIL — `skltn_obs::pricing` module doesn't exist yet.

- [ ] **Step 3: Write the pricing implementation**

`crates/skltn-obs/src/pricing.rs`:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ModelRates {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_write_per_mtok: f64,
    pub cache_read_per_mtok: f64,
}

impl ModelRates {
    pub fn zero() -> Self {
        Self {
            input_per_mtok: 0.0,
            output_per_mtok: 0.0,
            cache_write_per_mtok: 0.0,
            cache_read_per_mtok: 0.0,
        }
    }
}

/// Rates as of 2026-03-16. Verify against https://docs.anthropic.com/en/docs/about-claude/models
/// before implementation — prices may have changed.
pub fn get_rates(model: &str) -> ModelRates {
    match model {
        m if m.contains("claude-opus-4") => ModelRates {
            input_per_mtok: 15.00,
            output_per_mtok: 75.00,
            cache_write_per_mtok: 18.75,
            cache_read_per_mtok: 1.50,
        },
        m if m.contains("claude-sonnet-4") => ModelRates {
            input_per_mtok: 3.00,
            output_per_mtok: 15.00,
            cache_write_per_mtok: 3.75,
            cache_read_per_mtok: 0.30,
        },
        m if m.contains("claude-haiku-4") => ModelRates {
            input_per_mtok: 0.80,
            output_per_mtok: 4.00,
            cache_write_per_mtok: 1.00,
            cache_read_per_mtok: 0.08,
        },
        m if m.contains("claude-3-7-sonnet") => ModelRates {
            input_per_mtok: 3.00,
            output_per_mtok: 15.00,
            cache_write_per_mtok: 3.75,
            cache_read_per_mtok: 0.30,
        },
        m if m.contains("claude-3-5-sonnet") => ModelRates {
            input_per_mtok: 3.00,
            output_per_mtok: 15.00,
            cache_write_per_mtok: 3.75,
            cache_read_per_mtok: 0.30,
        },
        m if m.contains("claude-3-5-haiku") => ModelRates {
            input_per_mtok: 0.80,
            output_per_mtok: 4.00,
            cache_write_per_mtok: 1.00,
            cache_read_per_mtok: 0.08,
        },
        _ => {
            tracing::warn!("Unknown model '{}', cost tracking will show $0.00", model);
            ModelRates::zero()
        }
    }
}

pub fn calculate_cost(
    input_tokens: usize,
    output_tokens: usize,
    cache_creation_input_tokens: usize,
    cache_read_input_tokens: usize,
    rates: &ModelRates,
) -> f64 {
    (input_tokens as f64 * rates.input_per_mtok
        + output_tokens as f64 * rates.output_per_mtok
        + cache_creation_input_tokens as f64 * rates.cache_write_per_mtok
        + cache_read_input_tokens as f64 * rates.cache_read_per_mtok)
        / 1_000_000.0
}
```

- [ ] **Step 4: Update lib.rs to export pricing module**

`crates/skltn-obs/src/lib.rs`:
```rust
pub mod pricing;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p skltn-obs --test pricing_test`
Expected: All 9 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/skltn-obs/src/pricing.rs crates/skltn-obs/src/main.rs crates/skltn-obs/tests/pricing_test.rs
git commit -m "feat(obs): implement pricing module with model rate lookup and cost calculation"
```

---

### Task 3: Define UsageRecord

**Files:**
- Create: `crates/skltn-obs/src/tracker.rs`
- Modify: `crates/skltn-obs/src/lib.rs` (add module declaration)

- [ ] **Step 1: Write the UsageRecord struct**

`crates/skltn-obs/src/tracker.rs`:
```rust
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

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

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p skltn-obs`
Expected: Successful compilation.

- [ ] **Step 3: Update lib.rs module declarations**

`crates/skltn-obs/src/lib.rs`:
```rust
pub mod pricing;
pub mod tracker;
```

- [ ] **Step 4: Commit**

```bash
git add crates/skltn-obs/src/tracker.rs crates/skltn-obs/src/lib.rs
git commit -m "feat(obs): define UsageRecord struct with serde and time serialization"
```

---

## Chunk 2: CostTracker and JSONL Persistence

### Task 4: Implement CostTracker with Broadcast and JSONL Writer

**Files:**
- Modify: `crates/skltn-obs/src/tracker.rs` (add CostTracker, spawn_jsonl_writer)
- Create: `crates/skltn-obs/tests/tracker_test.rs`

- [ ] **Step 1: Write the failing tests**

`crates/skltn-obs/tests/tracker_test.rs`:
```rust
use skltn_obs::tracker::{CostTracker, UsageRecord};
use std::path::PathBuf;
use time::OffsetDateTime;

fn sample_record(model: &str) -> UsageRecord {
    UsageRecord {
        timestamp: OffsetDateTime::now_utc(),
        model: model.to_string(),
        input_tokens: 1000,
        output_tokens: 500,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 200,
        cost_usd: 0.05,
    }
}

#[tokio::test]
async fn test_record_stores_in_memory() {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;
    let record = sample_record("claude-sonnet-4-6");

    tracker.record(record.clone()).await;

    let records = tracker.records().await;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].model, "claude-sonnet-4-6");
}

#[tokio::test]
async fn test_multiple_records() {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;

    tracker.record(sample_record("claude-sonnet-4-6")).await;
    tracker.record(sample_record("claude-opus-4-6")).await;
    tracker.record(sample_record("claude-haiku-4-5")).await;

    let records = tracker.records().await;
    assert_eq!(records.len(), 3);
}

#[tokio::test]
async fn test_broadcast_receives_records() {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;
    let mut rx = tracker.subscribe().await;

    let record = sample_record("claude-opus-4-6");
    tracker.record(record.clone()).await;

    let received = rx.recv().await.unwrap();
    assert_eq!(received.model, "claude-opus-4-6");
}

#[tokio::test]
async fn test_jsonl_file_written() {
    let dir = tempfile::tempdir().unwrap();
    let jsonl_path = dir.path().join("usage.jsonl");
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;

    tracker.record(sample_record("claude-sonnet-4-6")).await;
    tracker.record(sample_record("claude-opus-4-6")).await;

    // Gracefully shut down — drains mpsc channel and awaits writer task
    tracker.shutdown().await;

    let contents = std::fs::read_to_string(&jsonl_path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 2, "Expected 2 JSONL lines, got {}", lines.len());

    // Each line should be valid JSON with the expected fields
    let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert!(parsed.get("model").is_some());
    assert!(parsed.get("cost_usd").is_some());
    assert!(parsed.get("timestamp").is_some());
}

#[tokio::test]
async fn test_jsonl_file_created_in_data_dir() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("subdir");
    // data_dir does not exist yet — CostTracker::new should create it
    let tracker = CostTracker::new(data_dir.clone()).await;

    tracker.record(sample_record("claude-sonnet-4-6")).await;
    tracker.shutdown().await;

    assert!(data_dir.join("usage.jsonl").exists());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p skltn-obs --test tracker_test`
Expected: FAIL — `CostTracker` struct doesn't exist yet (only `UsageRecord` is defined).

- [ ] **Step 3: Write the CostTracker implementation**

Add to `crates/skltn-obs/src/tracker.rs` (below the existing `UsageRecord` definition):
```rust
use std::path::PathBuf;
use tokio::sync::{broadcast, mpsc, Mutex};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::task::JoinHandle;

/// CostTracker wraps its internals in Arc<Mutex<...>> so it can be cheaply cloned
/// into axum State. This is a deliberate simplification over the spec's
/// `Arc<Mutex<CostTracker>>` pattern — the synchronization is internal.
/// Uses tokio::sync::Mutex (async-aware) rather than std::sync::Mutex.
#[derive(Clone)]
pub struct CostTracker {
    inner: Arc<Mutex<CostTrackerInner>>,
    writer_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

struct CostTrackerInner {
    records: Vec<UsageRecord>,
    file_writer: mpsc::Sender<UsageRecord>,
    broadcast: broadcast::Sender<UsageRecord>,
}

impl CostTracker {
    pub async fn new(data_dir: PathBuf) -> Self {
        // Ensure data directory exists with restrictive permissions
        tokio::fs::create_dir_all(&data_dir).await.expect("failed to create data directory");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o700);
            tokio::fs::set_permissions(&data_dir, perms).await.ok();
        }

        let (file_tx, file_rx) = mpsc::channel::<UsageRecord>(256);
        let (broadcast_tx, _) = broadcast::channel::<UsageRecord>(64);

        let handle = spawn_jsonl_writer(data_dir.join("usage.jsonl"), file_rx);

        Self {
            inner: Arc::new(Mutex::new(CostTrackerInner {
                records: Vec::new(),
                file_writer: file_tx,
                broadcast: broadcast_tx,
            })),
            writer_handle: Arc::new(Mutex::new(Some(handle))),
        }
    }

    pub async fn record(&self, record: UsageRecord) {
        let inner = &mut *self.inner.lock().await;
        inner.records.push(record.clone());
        // Ignore send errors — no subscribers is fine
        let _ = inner.broadcast.send(record.clone());
        // Send to file writer — await ensures no records are lost under backpressure
        if let Err(e) = inner.file_writer.send(record).await {
            tracing::error!("JSONL writer channel closed, record lost: {e}");
        }
    }

    pub async fn records(&self) -> Vec<UsageRecord> {
        self.inner.lock().await.records.clone()
    }

    pub async fn subscribe(&self) -> broadcast::Receiver<UsageRecord> {
        self.inner.lock().await.broadcast.subscribe()
    }

    /// Returns (records snapshot, broadcast receiver) under the same lock.
    /// This is the critical invariant for gap-free WebSocket replay-to-live handoff.
    pub async fn snapshot_and_subscribe(&self) -> (Vec<UsageRecord>, broadcast::Receiver<UsageRecord>) {
        let inner = self.inner.lock().await;
        (inner.records.clone(), inner.broadcast.subscribe())
    }

    /// Gracefully shuts down the JSONL writer: drops the mpsc sender so the writer
    /// task drains remaining records, then awaits the task to completion.
    pub async fn shutdown(self) {
        // Drop the inner (which holds the mpsc Sender), causing the writer to drain
        drop(self.inner);
        // Await the writer task to ensure all records are flushed to disk
        if let Some(handle) = self.writer_handle.lock().await.take() {
            let _ = handle.await;
        }
    }
}

fn spawn_jsonl_writer(path: PathBuf, mut rx: mpsc::Receiver<UsageRecord>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut opts = tokio::fs::OpenOptions::new();
        opts.append(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600); // Restrictive permissions — usage data is private
        }
        let mut file = opts.open(&path)
            .await
            .expect("failed to open JSONL file");

        while let Some(record) = rx.recv().await {
            match serde_json::to_string(&record) {
                Ok(json) => {
                    let line = format!("{json}\n");
                    if let Err(e) = file.write_all(line.as_bytes()).await {
                        tracing::error!("Failed to write JSONL record: {e}");
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to serialize UsageRecord: {e}");
                }
            }
        }
        // Channel closed — drain complete. Flush to ensure all bytes are written.
        if let Err(e) = file.flush().await {
            tracing::error!("Failed to flush JSONL file: {e}");
        }
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p skltn-obs --test tracker_test`
Expected: All 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/skltn-obs/src/tracker.rs crates/skltn-obs/tests/tracker_test.rs
git commit -m "feat(obs): implement CostTracker with broadcast, JSONL persistence, and snapshot_and_subscribe"
```

---

## Chunk 3: Proxy Handler and Non-Streaming Skimmer

### Task 5: Implement Non-Streaming Response Skimmer

**Files:**
- Create: `crates/skltn-obs/src/skim.rs`
- Modify: `crates/skltn-obs/src/lib.rs` (add module declaration)

- [ ] **Step 1: Write the non-streaming skimmer**

`crates/skltn-obs/src/skim.rs`:
```rust
use crate::pricing::{calculate_cost, get_rates};
use crate::tracker::{CostTracker, UsageRecord};
use axum::body::Body;
use axum::response::Response;
use http::response::Parts;
use time::OffsetDateTime;

/// Extracts usage from a buffered non-streaming JSON response.
/// Returns the original bytes unchanged as an axum Response, and records usage if found.
pub async fn skim_nonstreaming(
    parts: Parts,
    body_bytes: bytes::Bytes,
    model: &str,
    tracker: &CostTracker,
) -> Response {
    // Try to extract usage from the response JSON
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
        if let Some(usage) = json.get("usage") {
            let input_tokens = usage.get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let output_tokens = usage.get("output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let cache_creation_input_tokens = usage.get("cache_creation_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let cache_read_input_tokens = usage.get("cache_read_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;

            let rates = get_rates(model);
            let cost = calculate_cost(
                input_tokens,
                output_tokens,
                cache_creation_input_tokens,
                cache_read_input_tokens,
                &rates,
            );

            let record = UsageRecord {
                timestamp: OffsetDateTime::now_utc(),
                model: model.to_string(),
                input_tokens,
                output_tokens,
                cache_creation_input_tokens,
                cache_read_input_tokens,
                cost_usd: cost,
            };

            tracker.record(record).await;
        }
    }

    // Reconstruct the response with original bytes unchanged
    let mut response = Response::new(Body::from(body_bytes));
    *response.status_mut() = parts.status;
    *response.headers_mut() = parts.headers;
    *response.version_mut() = parts.version;
    response
}
```

- [ ] **Step 2: Update lib.rs module declarations**

Add `pub mod skim;` to `crates/skltn-obs/src/lib.rs`:
```rust
pub mod pricing;
pub mod skim;
pub mod tracker;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p skltn-obs`
Expected: Successful compilation.

- [ ] **Step 4: Commit**

```bash
git add crates/skltn-obs/src/skim.rs crates/skltn-obs/src/lib.rs
git commit -m "feat(obs): implement non-streaming response skimmer with usage extraction"
```

---

### Task 6: Implement Proxy Handler

**Files:**
- Create: `crates/skltn-obs/src/proxy.rs`
- Modify: `crates/skltn-obs/src/lib.rs` (add module declaration)

- [ ] **Step 1: Write the proxy handler**

`crates/skltn-obs/src/proxy.rs`:
```rust
use axum::body::Body;
use axum::extract::{Request, State};
use axum::response::{IntoResponse, Response};
use http::header::{HOST, TRANSFER_ENCODING};
use http::uri::Uri;

use crate::skim::skim_nonstreaming;
use crate::tracker::CostTracker;

#[derive(Clone)]
pub struct AppState {
    pub client: reqwest::Client,
    pub upstream: String,
    pub tracker: CostTracker,
}

/// Catch-all proxy handler. Forwards all requests to the upstream Anthropic API.
pub async fn proxy_handler(
    State(state): State<AppState>,
    req: Request,
) -> Result<Response, Response> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(|q| format!("?{q}")).unwrap_or_default();
    let upstream_url = format!("{}{path}{query}", state.upstream);

    // SECURITY: Log method and path only — never log headers (they contain x-api-key)
    tracing::debug!(%method, %path, "Proxying request");

    // Extract model from POST /v1/messages request body
    let headers = req.headers().clone();
    let body_bytes = axum::body::to_bytes(req.into_body(), 200 * 1024 * 1024)
        .await
        .map_err(|e| {
            tracing::error!("Failed to read request body: {e}");
            (http::StatusCode::BAD_REQUEST, "Failed to read request body").into_response()
        })?;

    let model = if method == http::Method::POST && path == "/v1/messages" {
        extract_model(&body_bytes)
    } else {
        None
    };

    // Build upstream request
    let mut upstream_req = state.client.request(method, &upstream_url);
    for (key, value) in headers.iter() {
        // Skip headers that reqwest manages or that shouldn't be forwarded
        if key == HOST || key == TRANSFER_ENCODING {
            continue;
        }
        upstream_req = upstream_req.header(key.as_str(), value);
    }
    upstream_req = upstream_req.body(body_bytes.to_vec());

    // Send upstream request
    let upstream_resp = upstream_req.send().await.map_err(|e| {
        tracing::error!("Upstream request failed: {e}");
        (http::StatusCode::BAD_GATEWAY, format!("Upstream error: {e}")).into_response()
    })?;

    // Copy response status and headers
    let status = upstream_resp.status();
    let resp_headers = upstream_resp.headers().clone();
    let content_type = resp_headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let is_streaming = content_type.contains("text/event-stream");

    if is_streaming {
        // Streaming path — handled in Task 8 (skim_streaming)
        // For now, forward the stream directly without skimming
        let stream = upstream_resp.bytes_stream();
        let body = Body::from_stream(stream);
        let mut response = Response::new(body);
        *response.status_mut() = status;
        *response.headers_mut() = resp_headers;
        Ok(response)
    } else {
        // Non-streaming path — buffer and skim
        let resp_bytes = upstream_resp.bytes().await.map_err(|e| {
            tracing::error!("Failed to read upstream response body: {e}");
            (http::StatusCode::BAD_GATEWAY, "Failed to read upstream response").into_response()
        })?;

        let mut parts = http::Response::new(()).into_parts().0;
        parts.status = status;
        parts.headers = resp_headers;

        if let Some(ref model) = model {
            Ok(skim_nonstreaming(parts, resp_bytes, model, &state.tracker).await)
        } else {
            // No model extracted (not a /v1/messages request or parse failed) — pass through
            let mut response = Response::new(Body::from(resp_bytes));
            *response.status_mut() = parts.status;
            *response.headers_mut() = parts.headers;
            Ok(response)
        }
    }
}

/// Extract the "model" field from a JSON request body.
/// Returns None if parsing fails — the request is still forwarded.
/// Regex for valid model names: alphanumeric, hyphens, dots, underscores.
static MODEL_NAME_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^[a-zA-Z0-9._-]+$").unwrap());

fn extract_model(body: &[u8]) -> Option<String> {
    match serde_json::from_slice::<serde_json::Value>(body) {
        Ok(json) => {
            let raw = json.get("model").and_then(|v| v.as_str())?;
            if MODEL_NAME_RE.is_match(raw) {
                Some(raw.to_string())
            } else {
                tracing::warn!("Invalid model name '{raw}', replacing with 'unknown'");
                Some("unknown".to_string())
            }
        }
        Err(e) => {
            tracing::warn!("Failed to parse request body for model extraction: {e}");
            None
        }
    }
}
```

- [ ] **Step 2: Update lib.rs module declarations**

`crates/skltn-obs/src/lib.rs`:
```rust
pub mod pricing;
pub mod proxy;
pub mod skim;
pub mod tracker;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p skltn-obs`
Expected: Successful compilation.

- [ ] **Step 4: Commit**

```bash
git add crates/skltn-obs/src/proxy.rs crates/skltn-obs/src/lib.rs
git commit -m "feat(obs): implement catch-all proxy handler with model extraction and non-streaming skimming"
```

---

### Task 7: End-to-End Non-Streaming Proxy Test

**Files:**
- Create: `crates/skltn-obs/tests/proxy_nonstreaming_test.rs`

- [ ] **Step 1: Write the end-to-end test with mock upstream**

`crates/skltn-obs/tests/proxy_nonstreaming_test.rs`:
```rust
use axum::routing::{any, post};
use axum::Router;
use skltn_obs::proxy::{proxy_handler, AppState};
use skltn_obs::tracker::CostTracker;
use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Mock Anthropic server that returns a canned /v1/messages response
async fn mock_messages_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "id": "msg_test123",
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": "Hello!"}],
        "model": "claude-sonnet-4-6-20260301",
        "usage": {
            "input_tokens": 100,
            "output_tokens": 50,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0
        }
    }))
}

/// Mock handler for /v1/models (no usage data)
async fn mock_models_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "data": [{"id": "claude-sonnet-4-6", "type": "model"}]
    }))
}

async fn start_mock_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .route("/v1/messages", post(mock_messages_handler))
        .route("/v1/models", any(mock_models_handler));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, handle)
}

async fn start_proxy(upstream_addr: SocketAddr) -> (SocketAddr, CostTracker) {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.into_path()).await;
    // Matches spec: redirect(Policy::none()), tcp_nodelay(true), timeout(300s)
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .tcp_nodelay(true)
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .unwrap();

    let state = AppState {
        client,
        upstream: format!("http://{upstream_addr}"),
        tracker: tracker.clone(),
    };

    let app = Router::new()
        .fallback(proxy_handler)
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, tracker)
}

#[tokio::test]
async fn test_nonstreaming_proxy_extracts_usage() {
    let (mock_addr, _mock_handle) = start_mock_server().await;
    let (proxy_addr, tracker) = start_proxy(mock_addr).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{proxy_addr}/v1/messages"))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": "claude-sonnet-4-6-20260301",
            "max_tokens": 100,
            "messages": [{"role": "user", "content": "Hi"}]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["id"], "msg_test123");
    assert_eq!(body["usage"]["input_tokens"], 100);

    // Verify usage was tracked
    // Small delay to ensure record is processed
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let records = tracker.records().await;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].model, "claude-sonnet-4-6-20260301");
    assert_eq!(records[0].input_tokens, 100);
    assert_eq!(records[0].output_tokens, 50);
    assert!(records[0].cost_usd > 0.0);
}

#[tokio::test]
async fn test_passthrough_non_message_endpoint() {
    let (mock_addr, _mock_handle) = start_mock_server().await;
    let (proxy_addr, tracker) = start_proxy(mock_addr).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{proxy_addr}/v1/models"))
        .header("x-api-key", "test-key")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("data").is_some());

    // No usage record should be created for /v1/models
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let records = tracker.records().await;
    assert_eq!(records.len(), 0);
}

#[tokio::test]
async fn test_proxy_returns_upstream_error_status() {
    // Mock server returns 400 for malformed requests
    let app = Router::new().fallback(|| async {
        (http::StatusCode::BAD_REQUEST, "bad request")
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mock_addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let (proxy_addr, tracker) = start_proxy(mock_addr).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{proxy_addr}/v1/messages"))
        .header("content-type", "application/json")
        .json(&serde_json::json!({"model": "claude-sonnet-4-6", "bad": true}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);

    // Error responses should not generate UsageRecords
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let records = tracker.records().await;
    assert_eq!(records.len(), 0, "Error responses should not create usage records");
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p skltn-obs --test proxy_nonstreaming_test`
Expected: All 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/skltn-obs/tests/proxy_nonstreaming_test.rs
git commit -m "test(obs): add end-to-end non-streaming proxy tests with mock upstream"
```

---

## Chunk 4: Streaming SSE Skimmer

### Task 8: Implement Streaming SSE Skimmer

**Files:**
- Modify: `crates/skltn-obs/src/skim.rs` (add `skim_streaming` function)

- [ ] **Step 1: Write the streaming skimmer**

Add to `crates/skltn-obs/src/skim.rs` (below the existing `skim_nonstreaming` function):
```rust
use futures::stream::StreamExt;
use tokio::sync::mpsc as tokio_mpsc;

/// Processes a streaming SSE response using the "background tee" approach.
/// Chunks are forwarded immediately to the client via a channel.
/// A background task clones each chunk, parses SSE events, and extracts usage data
/// from `message_start` and `message_delta` events.
pub fn skim_streaming(
    parts: Parts,
    upstream_stream: impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
    model: String,
    tracker: CostTracker,
) -> Response {
    let (tx, rx) = tokio_mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(64);

    // Background task: read from upstream, forward to client channel, parse SSE events
    tokio::spawn(async move {
        let mut stream = std::pin::pin!(upstream_stream);
        let mut buffer = String::new();
        const MAX_SSE_BUFFER: usize = 10 * 1024 * 1024; // 10 MB
        let mut input_tokens: Option<usize> = None;
        let mut output_tokens: Option<usize> = None;
        let mut cache_creation_input_tokens: usize = 0;
        let mut cache_read_input_tokens: usize = 0;
        let mut saw_message_start = false;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    // Forward clone to client immediately
                    if tx.send(Ok(chunk.clone())).await.is_err() {
                        // Client disconnected
                        return;
                    }

                    // Append to SSE parse buffer
                    if let Ok(text) = std::str::from_utf8(&chunk) {
                        buffer.push_str(text);
                    }

                    // Guard against unbounded buffer growth (malformed/malicious upstream)
                    if buffer.len() > MAX_SSE_BUFFER {
                        tracing::warn!("SSE buffer exceeded 10 MB without event boundary, discarding");
                        buffer.clear();
                        // Continue forwarding chunks to client, but stop parsing
                        while let Some(chunk_result) = stream.next().await {
                            if let Ok(chunk) = chunk_result {
                                let _ = tx.send(Ok(chunk)).await;
                            }
                        }
                        return; // No UsageRecord generated
                    }

                    // Parse complete SSE events (delimited by \n\n)
                    while let Some(boundary) = buffer.find("\n\n") {
                        let event_block = buffer[..boundary].to_string();
                        buffer = buffer[boundary + 2..].to_string();

                        // Parse SSE event: look for "event:" and "data:" lines
                        let mut event_type = None;
                        let mut event_data = None;
                        for line in event_block.lines() {
                            if let Some(t) = line.strip_prefix("event: ") {
                                event_type = Some(t.to_string());
                            } else if let Some(d) = line.strip_prefix("data: ") {
                                event_data = Some(d.to_string());
                            }
                        }

                        if let (Some(ref etype), Some(ref data)) = (&event_type, &event_data) {
                            match etype.as_str() {
                                "message_start" => {
                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                        if let Some(usage) = json.pointer("/message/usage") {
                                            input_tokens = usage.get("input_tokens")
                                                .and_then(|v| v.as_u64())
                                                .map(|v| v as usize);
                                            cache_creation_input_tokens = usage
                                                .get("cache_creation_input_tokens")
                                                .and_then(|v| v.as_u64())
                                                .unwrap_or(0) as usize;
                                            cache_read_input_tokens = usage
                                                .get("cache_read_input_tokens")
                                                .and_then(|v| v.as_u64())
                                                .unwrap_or(0) as usize;
                                            saw_message_start = true;
                                        }
                                    }
                                }
                                "message_delta" => {
                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                        if let Some(usage) = json.get("usage") {
                                            output_tokens = usage.get("output_tokens")
                                                .and_then(|v| v.as_u64())
                                                .map(|v| v as usize);
                                        }
                                    }
                                }
                                "message_stop" => {
                                    // Stream complete — emit record if we have both parts
                                }
                                _ => {
                                    // Ignore other event types (content_block_start, etc.)
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Upstream stream error: {e}");
                    break;
                }
            }
        }

        // Stream ended — emit UsageRecord if we have complete data
        if saw_message_start {
            if let (Some(input), Some(output)) = (input_tokens, output_tokens) {
                let rates = crate::pricing::get_rates(&model);
                let cost = crate::pricing::calculate_cost(
                    input,
                    output,
                    cache_creation_input_tokens,
                    cache_read_input_tokens,
                    &rates,
                );
                let record = UsageRecord {
                    timestamp: OffsetDateTime::now_utc(),
                    model,
                    input_tokens: input,
                    output_tokens: output,
                    cache_creation_input_tokens,
                    cache_read_input_tokens,
                    cost_usd: cost,
                };
                tracker.record(record).await;
            } else {
                tracing::warn!(
                    "Incomplete streaming response: message_start received but missing {}",
                    if input_tokens.is_none() { "input_tokens" } else { "output_tokens (no message_delta)" }
                );
            }
        }
    });

    // Build response that streams from the channel
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    let body = Body::from_stream(stream);
    let mut response = Response::new(body);
    *response.status_mut() = parts.status;
    *response.headers_mut() = parts.headers;
    response
}
```

- [ ] **Step 2: Add `tokio-stream` dependency**

Add to `crates/skltn-obs/Cargo.toml` under `[dependencies]`:
```toml
tokio-stream = "0.1"
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p skltn-obs`
Expected: Successful compilation.

- [ ] **Step 4: Commit**

```bash
git add crates/skltn-obs/src/skim.rs crates/skltn-obs/Cargo.toml
git commit -m "feat(obs): implement streaming SSE skimmer with background tee and event parsing"
```

---

### Task 9: Wire Streaming Path into Proxy Handler

**Files:**
- Modify: `crates/skltn-obs/src/proxy.rs` (replace streaming placeholder with `skim_streaming` call)

- [ ] **Step 1: Update the streaming branch in proxy_handler**

In `crates/skltn-obs/src/proxy.rs`, replace the streaming branch:
```rust
    if is_streaming {
        // Streaming path — handled in Task 8 (skim_streaming)
        // For now, forward the stream directly without skimming
        let stream = upstream_resp.bytes_stream();
        let body = Body::from_stream(stream);
        let mut response = Response::new(body);
        *response.status_mut() = status;
        *response.headers_mut() = resp_headers;
        Ok(response)
    }
```

Replace with:
```rust
    if is_streaming {
        let mut parts = http::Response::new(()).into_parts().0;
        parts.status = status;
        parts.headers = resp_headers;

        if let Some(model) = model {
            Ok(crate::skim::skim_streaming(
                parts,
                upstream_resp.bytes_stream(),
                model,
                state.tracker.clone(),
            ))
        } else {
            // No model — forward stream without skimming
            let body = Body::from_stream(upstream_resp.bytes_stream());
            let mut response = Response::new(body);
            *response.status_mut() = parts.status;
            *response.headers_mut() = parts.headers;
            Ok(response)
        }
    }
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p skltn-obs`
Expected: Successful compilation.

- [ ] **Step 3: Commit**

```bash
git add crates/skltn-obs/src/proxy.rs
git commit -m "feat(obs): wire streaming SSE skimmer into proxy handler"
```

---

### Task 10: End-to-End Streaming Proxy Test

**Files:**
- Create: `crates/skltn-obs/tests/proxy_streaming_test.rs`

- [ ] **Step 1: Write the streaming end-to-end test**

`crates/skltn-obs/tests/proxy_streaming_test.rs`:
```rust
use axum::body::Body;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use futures::stream;
use skltn_obs::proxy::{proxy_handler, AppState};
use skltn_obs::tracker::CostTracker;
use std::net::SocketAddr;
use tokio::net::TcpListener;

fn sse_body_from_strings(chunks: Vec<String>) -> Body {
    let byte_stream = stream::iter(
        chunks
            .into_iter()
            .map(|s| Ok::<bytes::Bytes, std::io::Error>(bytes::Bytes::from(s)))
    );
    Body::from_stream(byte_stream)
}

/// Build a mock SSE response with message_start, content, message_delta, message_stop
fn mock_sse_body() -> Body {
    sse_body_from_strings(vec![
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_test\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-4-6-20260301\",\"usage\":{\"input_tokens\":150,\"cache_creation_input_tokens\":0,\"cache_read_input_tokens\":50}}}\n\n".to_string(),
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n".to_string(),
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello!\"}}\n\n".to_string(),
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n".to_string(),
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":75}}\n\n".to_string(),
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_string(),
    ])
}

async fn mock_streaming_handler() -> Response {
    let body = mock_sse_body();
    (
        [("content-type", "text/event-stream")],
        body,
    ).into_response()
}

async fn start_mock_sse_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .route("/v1/messages", post(mock_streaming_handler));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, handle)
}

async fn start_proxy(upstream_addr: SocketAddr) -> (SocketAddr, CostTracker) {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.into_path()).await;
    // Matches spec: redirect(Policy::none()), tcp_nodelay(true), timeout(300s)
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .tcp_nodelay(true)
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .unwrap();

    let state = AppState {
        client,
        upstream: format!("http://{upstream_addr}"),
        tracker: tracker.clone(),
    };

    let app = Router::new()
        .fallback(proxy_handler)
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, tracker)
}

#[tokio::test]
async fn test_streaming_proxy_extracts_usage() {
    let (mock_addr, _handle) = start_mock_sse_server().await;
    let (proxy_addr, tracker) = start_proxy(mock_addr).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{proxy_addr}/v1/messages"))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": "claude-sonnet-4-6-20260301",
            "max_tokens": 100,
            "stream": true,
            "messages": [{"role": "user", "content": "Hi"}]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    // Consume the full SSE stream
    let body_text = resp.text().await.unwrap();
    assert!(body_text.contains("message_start"));
    assert!(body_text.contains("message_delta"));
    assert!(body_text.contains("message_stop"));

    // Wait for background task to finish processing
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let records = tracker.records().await;
    assert_eq!(records.len(), 1, "Expected 1 usage record, got {}", records.len());
    assert_eq!(records[0].model, "claude-sonnet-4-6-20260301");
    assert_eq!(records[0].input_tokens, 150);
    assert_eq!(records[0].output_tokens, 75);
    assert_eq!(records[0].cache_read_input_tokens, 50);
    assert!(records[0].cost_usd > 0.0);
}

/// Mock handler that returns SSE events split across chunk boundaries
async fn mock_split_chunks_handler() -> Response {
    let body = sse_body_from_strings(vec![
        // First chunk contains partial message_start event (no \n\n yet)
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":200,\"cache_creation_".to_string(),
        // Second chunk completes message_start and includes content events
        "input_tokens\":0,\"cache_read_input_tokens\":0}}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n".to_string(),
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":30}}\n\n".to_string(),
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_string(),
    ]);
    ([("content-type", "text/event-stream")], body).into_response()
}

/// Mock handler that returns only message_start then ends (incomplete stream)
async fn mock_incomplete_stream_handler() -> Response {
    let body = sse_body_from_strings(vec![
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":100,\"cache_creation_input_tokens\":0,\"cache_read_input_tokens\":0}}}\n\n".to_string(),
        // Stream ends abruptly — no message_delta or message_stop
    ]);
    ([("content-type", "text/event-stream")], body).into_response()
}

#[tokio::test]
async fn test_streaming_split_chunks() {
    // Test SSE events split across chunk boundaries
    let app = Router::new().route("/v1/messages", post(mock_split_chunks_handler));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mock_addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });

    let (proxy_addr, tracker) = start_proxy(mock_addr).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{proxy_addr}/v1/messages"))
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": "claude-sonnet-4-6-20260301",
            "max_tokens": 100,
            "stream": true,
            "messages": [{"role": "user", "content": "Hi"}]
        }))
        .send()
        .await
        .unwrap();

    let _ = resp.text().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let records = tracker.records().await;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].input_tokens, 200);
    assert_eq!(records[0].output_tokens, 30);
}

#[tokio::test]
async fn test_streaming_incomplete_discards_record() {
    // message_start sent but stream ends before message_delta
    let app = Router::new().route("/v1/messages", post(mock_incomplete_stream_handler));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mock_addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });

    let (proxy_addr, tracker) = start_proxy(mock_addr).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{proxy_addr}/v1/messages"))
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": "claude-sonnet-4-6-20260301",
            "max_tokens": 100,
            "stream": true,
            "messages": [{"role": "user", "content": "Hi"}]
        }))
        .send()
        .await
        .unwrap();

    let _ = resp.text().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // No record should be created for incomplete stream
    let records = tracker.records().await;
    assert_eq!(records.len(), 0, "Expected no records for incomplete stream, got {}", records.len());
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p skltn-obs --test proxy_streaming_test`
Expected: All 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/skltn-obs/tests/proxy_streaming_test.rs
git commit -m "test(obs): add end-to-end streaming SSE proxy tests including split chunks and incomplete streams"
```

---

## Chunk 5: WebSocket Endpoint

### Task 11: Implement WebSocket Handler

**Files:**
- Create: `crates/skltn-obs/src/ws.rs`
- Modify: `crates/skltn-obs/src/lib.rs` (add module declaration)

- [ ] **Step 1: Write the WebSocket handler**

`crates/skltn-obs/src/ws.rs`:
```rust
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use tokio::sync::broadcast;

use crate::proxy::AppState;
use crate::tracker::CostTracker;

pub async fn ws_handler(
    headers: axum::http::HeaderMap,
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, StatusCode> {
    // Validate Origin header to prevent cross-site WebSocket hijacking.
    // Accept: no Origin (non-browser clients), or localhost Origins.
    if let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) {
        let allowed = origin.starts_with("http://localhost:")
            || origin.starts_with("http://127.0.0.1:")
            || origin.starts_with("http://[::1]:");
        if !allowed {
            tracing::warn!("Rejected WebSocket connection from origin: {origin}");
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(ws.on_upgrade(|socket| handle_ws(socket, state.tracker)))
}

async fn handle_ws(mut socket: WebSocket, tracker: CostTracker) {
    // 1. Replay existing records + subscribe under the same lock (gap-free invariant)
    let (records, mut rx) = tracker.snapshot_and_subscribe().await;

    for record in records {
        let msg = match serde_json::to_string(&record) {
            Ok(json) => json,
            Err(_) => continue,
        };
        if socket.send(Message::Text(msg.into())).await.is_err() {
            return;
        }
    }

    // 2. Stream live records
    loop {
        match rx.recv().await {
            Ok(record) => {
                let msg = match serde_json::to_string(&record) {
                    Ok(json) => json,
                    Err(_) => continue,
                };
                if socket.send(Message::Text(msg.into())).await.is_err() {
                    return;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("WebSocket client lagged by {n} messages, dropping connection");
                return;
            }
            Err(broadcast::error::RecvError::Closed) => {
                return;
            }
        }
    }
}
```

- [ ] **Step 2: Update lib.rs module declarations**

`crates/skltn-obs/src/lib.rs`:
```rust
pub mod pricing;
pub mod proxy;
pub mod skim;
pub mod tracker;
pub mod ws;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p skltn-obs`
Expected: Successful compilation.

- [ ] **Step 4: Commit**

```bash
git add crates/skltn-obs/src/ws.rs crates/skltn-obs/src/lib.rs
git commit -m "feat(obs): implement WebSocket handler with replay-to-live handoff"
```

---

### Task 12: WebSocket Tests

**Files:**
- Create: `crates/skltn-obs/tests/ws_test.rs`

- [ ] **Step 1: Write the WebSocket tests**

`crates/skltn-obs/tests/ws_test.rs`:
```rust
use axum::routing::get;
use axum::Router;
use futures::{SinkExt, StreamExt};
use skltn_obs::proxy::AppState;
use skltn_obs::tracker::{CostTracker, UsageRecord};
use skltn_obs::ws::ws_handler;
use std::net::SocketAddr;
use time::OffsetDateTime;
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;

fn sample_record(model: &str, input: usize, output: usize) -> UsageRecord {
    UsageRecord {
        timestamp: OffsetDateTime::now_utc(),
        model: model.to_string(),
        input_tokens: input,
        output_tokens: output,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
        cost_usd: 0.01,
    }
}

fn make_app_state(tracker: CostTracker) -> AppState {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    AppState {
        client,
        upstream: "http://unused".to_string(),
        tracker,
    }
}

async fn start_ws_server(tracker: CostTracker) -> SocketAddr {
    let state = make_app_state(tracker);
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test]
async fn test_ws_replay_existing_records() {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;

    // Add records before WebSocket connects
    tracker.record(sample_record("model-a", 100, 50)).await;
    tracker.record(sample_record("model-b", 200, 100)).await;

    let addr = start_ws_server(tracker).await;
    let (mut ws, _) = connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();

    // Should receive 2 replayed records
    let msg1 = ws.next().await.unwrap().unwrap();
    let record1: serde_json::Value = serde_json::from_str(&msg1.into_text().unwrap()).unwrap();
    assert_eq!(record1["model"], "model-a");

    let msg2 = ws.next().await.unwrap().unwrap();
    let record2: serde_json::Value = serde_json::from_str(&msg2.into_text().unwrap()).unwrap();
    assert_eq!(record2["model"], "model-b");
}

#[tokio::test]
async fn test_ws_live_records() {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;

    let addr = start_ws_server(tracker.clone()).await;
    let (mut ws, _) = connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();

    // Give the WebSocket handler time to finish replay (empty) and subscribe
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Add a record after WebSocket is connected
    tracker.record(sample_record("model-live", 300, 150)).await;

    let msg = ws.next().await.unwrap().unwrap();
    let record: serde_json::Value = serde_json::from_str(&msg.into_text().unwrap()).unwrap();
    assert_eq!(record["model"], "model-live");
    assert_eq!(record["input_tokens"], 300);
}

#[tokio::test]
async fn test_ws_multiple_clients() {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;

    let addr = start_ws_server(tracker.clone()).await;

    let (mut ws1, _) = connect_async(format!("ws://{addr}/ws")).await.unwrap();
    let (mut ws2, _) = connect_async(format!("ws://{addr}/ws")).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    tracker.record(sample_record("model-broadcast", 400, 200)).await;

    let msg1 = ws1.next().await.unwrap().unwrap();
    let r1: serde_json::Value = serde_json::from_str(&msg1.into_text().unwrap()).unwrap();
    assert_eq!(r1["model"], "model-broadcast");

    let msg2 = ws2.next().await.unwrap().unwrap();
    let r2: serde_json::Value = serde_json::from_str(&msg2.into_text().unwrap()).unwrap();
    assert_eq!(r2["model"], "model-broadcast");
}

#[tokio::test]
async fn test_ws_replay_then_live() {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;

    // Add a record before connecting
    tracker.record(sample_record("model-before", 100, 50)).await;

    let addr = start_ws_server(tracker.clone()).await;
    let (mut ws, _) = connect_async(format!("ws://{addr}/ws")).await.unwrap();

    // First message should be the replayed record
    let msg1 = ws.next().await.unwrap().unwrap();
    let r1: serde_json::Value = serde_json::from_str(&msg1.into_text().unwrap()).unwrap();
    assert_eq!(r1["model"], "model-before");

    // Now add a live record
    tracker.record(sample_record("model-after", 200, 100)).await;

    let msg2 = ws.next().await.unwrap().unwrap();
    let r2: serde_json::Value = serde_json::from_str(&msg2.into_text().unwrap()).unwrap();
    assert_eq!(r2["model"], "model-after");
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p skltn-obs --test ws_test`
Expected: All 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/skltn-obs/tests/ws_test.rs
git commit -m "test(obs): add WebSocket replay, live broadcast, and multi-client tests"
```

---

## Chunk 6: Server Bootstrap and Graceful Shutdown

### Task 13: Implement main.rs with CLI Args and Server Bootstrap

**Files:**
- Modify: `crates/skltn-obs/src/main.rs` (replace stub with full implementation)

- [ ] **Step 1: Write the main.rs implementation**

`crates/skltn-obs/src/main.rs`:
```rust
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use axum::routing::get;
use axum::Router;
use tokio::net::TcpListener;
use tokio::signal;

use skltn_obs::proxy::AppState;
use skltn_obs::tracker::CostTracker;
use skltn_obs::{proxy, ws};

#[derive(Parser)]
#[command(name = "skltn-obs", about = "Anthropic API observability proxy")]
struct Cli {
    /// Local port to listen on
    #[arg(long, default_value = "8080")]
    port: u16,

    /// Bind address (default: 127.0.0.1 — localhost only for security)
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,

    /// Required when --bind is non-loopback (safety gate for API key exposure)
    #[arg(long, default_value_t = false)]
    allow_external: bool,

    /// Anthropic API base URL
    #[arg(long, default_value = "https://api.anthropic.com")]
    upstream: String,

    /// Data directory for JSONL persistence
    #[arg(long, default_value_os_t = default_data_dir())]
    data_dir: PathBuf,
}

fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".skltn")
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Validate --upstream: must be HTTPS unless loopback
    if let Ok(url) = url::Url::parse(&cli.upstream) {
        let is_loopback_host = matches!(
            url.host_str(),
            Some("127.0.0.1") | Some("localhost") | Some("::1") | Some("[::1]")
        );
        if url.scheme() != "https" && !is_loopback_host {
            eprintln!("Error: --upstream must use HTTPS for non-loopback hosts. Got: {}", cli.upstream);
            std::process::exit(1);
        }
        if let Some(host) = url.host_str() {
            if !host.contains("anthropic.com") && !is_loopback_host {
                tracing::warn!(
                    "Upstream '{}' is not an Anthropic endpoint — API key will be sent to this server.",
                    cli.upstream
                );
            }
        }
    }

    tracing::info!(
        port = cli.port,
        upstream = %cli.upstream,
        data_dir = %cli.data_dir.display(),
        "Starting skltn-obs proxy"
    );

    let tracker = CostTracker::new(cli.data_dir).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .tcp_nodelay(true)
        .timeout(Duration::from_secs(300))
        .build()
        .expect("failed to build HTTP client");

    let state = AppState {
        client,
        upstream: cli.upstream,
        tracker: tracker.clone(),
    };

    let app = Router::new()
        .route("/ws", get(ws::ws_handler))
        .fallback(proxy::proxy_handler)
        .with_state(state)
        .layer(axum::extract::DefaultBodyLimit::max(200 * 1024 * 1024)); // 200 MB

    let addr: SocketAddr = format!("{}:{}", cli.bind, cli.port)
        .parse()
        .expect("invalid bind address");

    // Refuse non-loopback bind unless --allow-external is passed
    if !addr.ip().is_loopback() {
        if !cli.allow_external {
            eprintln!(
                "Error: Binding to non-loopback address {} requires --allow-external flag.\n\
                 API keys will be transmitted in cleartext HTTP over the network.",
                addr.ip()
            );
            std::process::exit(1);
        }
        eprintln!(
            "⚠ WARNING: Binding to {} — API keys will be transmitted in cleartext HTTP over the network. \
             Only use this on trusted networks.",
            addr
        );
    }

    let listener = TcpListener::bind(addr).await.expect("failed to bind address");
    tracing::info!(%addr, "Proxy listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    tracing::info!("Proxy shut down, draining JSONL writer...");
    // Gracefully shutdown: drops the mpsc sender, waits for writer task to drain and flush
    tracker.shutdown().await;
    tracing::info!("Shutdown complete");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => { tracing::info!("Received SIGINT"); }
        _ = terminate => { tracing::info!("Received SIGTERM"); }
    }
}
```

- [ ] **Step 2: Add `dirs` dependency**

Add to `crates/skltn-obs/Cargo.toml` under `[dependencies]`:
```toml
dirs = "6"
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p skltn-obs`
Expected: Successful compilation.

- [ ] **Step 4: Verify the binary runs and prints help**

Run: `cargo run -p skltn-obs -- --help`
Expected: Help output showing `--port`, `--upstream`, and `--data-dir` options.

- [ ] **Step 5: Commit**

```bash
git add crates/skltn-obs/src/main.rs crates/skltn-obs/Cargo.toml
git commit -m "feat(obs): implement main.rs with CLI args, server bootstrap, and graceful shutdown"
```

---

### Task 14: Run All Tests and Final Verification

**Files:**
- No new files — verification only

- [ ] **Step 1: Run the full test suite**

Run: `cargo test -p skltn-obs`
Expected: All tests pass across all test files (pricing_test, tracker_test, proxy_nonstreaming_test, proxy_streaming_test, ws_test).

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -p skltn-obs -- -D warnings`
Expected: No warnings or errors.

- [ ] **Step 3: Verify clean build**

Run: `cargo build -p skltn-obs --release`
Expected: Successful release build.

- [ ] **Step 4: Commit any clippy fixes if needed**

If clippy produced fixable warnings in previous steps:
```bash
git add crates/skltn-obs/
git commit -m "fix(obs): address clippy warnings"
```

If no fixes needed, skip this step.
