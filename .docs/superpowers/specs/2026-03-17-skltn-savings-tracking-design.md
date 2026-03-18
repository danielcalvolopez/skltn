# Skeletonization Savings Tracking

## Problem

The observability dashboard shows session cost and Anthropic prompt-cache savings, but has no visibility into the token reduction achieved by skltn's skeletonization engine. Users cannot see what their session would have cost without skltn.

## Solution

A shared-state JSONL file written by `skltn-mcp` and read by `skltn-obs` that records per-file skeletonization savings. The obs proxy watches the file, broadcasts savings records to the dashboard via WebSocket, and the dashboard displays an accumulated "SKLTN SAVINGS" metric in the MetricsBar.

## Architecture

```
skltn-mcp                          skltn-obs                        Dashboard
┌──────────────┐                  ┌──────────────────┐            ┌──────────────┐
│read_skeleton  │ append           │ SavingsTracker   │  WS        │MetricsBar    │
│  → savings.   │───────────────→ │  file watch      │──────────→ │  SKLTN       │
│    jsonl      │                  │  broadcast       │            │  SAVINGS     │
└──────────────┘                  └──────────────────┘            └──────────────┘
```

**Data flow:** MCP appends → obs watches + broadcasts → dashboard accumulates and displays.

**Decoupling:** MCP and obs share only a file path convention (`~/.skltn/savings.jsonl`). No direct IPC, no dependency direction between the two processes.

## Data Model

### SavingsRecord

Written as one JSON line per skeletonization event:

```json
{
  "timestamp": "2026-03-17T14:30:00Z",
  "file": "src/engine.rs",
  "language": "rust",
  "original_tokens": 4200,
  "skeleton_tokens": 850,
  "saved_tokens": 3350
}
```

Fields:
- `timestamp` — when skeletonization occurred (UTC, OffsetDateTime)
- `file` — relative file path (never absolute, per security rules)
- `language` — detected language identifier
- `original_tokens` — token count of the full source file
- `skeleton_tokens` — token count of the skeleton output
- `saved_tokens` — `original_tokens - skeleton_tokens` (precomputed)

Only files that were actually skeletonized are logged. Files served full (under threshold or cache-aware) produce no savings record.

## Components

### 1. MCP Writer (skltn-mcp)

**Location:** `crates/skltn-mcp/src/tools/read_skeleton.rs`

**Behavior:** After `should_skeletonize()` returns true and the engine produces a skeleton:

1. Token count on original source is already available from the budget check.
2. Count tokens on the skeleton output.
3. Construct a `SavingsRecord`.
4. Append as a single JSON line to `savings.jsonl`, flush.

**File path resolution:** `dirs::home_dir().join(".skltn/savings.jsonl")` — matches the existing `skltn-obs` convention where `default_data_dir()` resolves to `~/.skltn/`. Directory creation happens once at MCP server startup, stored in shared state alongside the existing `SessionTracker`. The resolved path must be canonicalized after directory creation per project security rules.

**Writer design:** Append-only, no locking needed (MCP is the sole writer). Open in append mode, write, flush. This runs within the existing `spawn_blocking` context in `read_skeleton_or_full`, so it adds negligible latency to the tool response (single line write + flush).

### 2. Obs File Watcher + Broadcast (skltn-obs)

**Location:** New module `crates/skltn-obs/src/savings.rs`

**Startup:**
1. Resolve `savings.jsonl` path via `dirs::home_dir().join(".skltn/savings.jsonl")` — same directory as `usage.jsonl`. Canonicalize after directory creation.
2. Create `~/.skltn/` directory if it doesn't exist (obs already does this for `usage.jsonl`).
3. **Truncate** `savings.jsonl` (or create empty) — start fresh each obs session, matching `CostTracker`'s behavior of starting with an empty `Vec<UsageRecord>`. This prevents stale savings from previous sessions appearing alongside a reset session cost.
4. Record file offset at 0 for subsequent tail reads.

**File watching:**
1. Spawn a tokio task using the `notify` crate to watch `savings.jsonl` for modifications.
2. On change event: seek to last known offset, read new lines, parse each as `SavingsRecord`.
3. Broadcast each new record via a `tokio::sync::broadcast` channel.
4. Update stored offset.

**SavingsTracker struct:** Mirrors `CostTracker` pattern:
- `records: Vec<SavingsRecord>` — in-memory history
- `broadcast: broadcast::Sender<SavingsRecord>` — live stream
- `snapshot_and_subscribe()` — returns history + receiver under same lock (gap-free handoff)

### 3. WebSocket Message Format Change

**Current format:** Raw `UsageRecord` JSON per message.

**New format:** Typed envelope:
```json
{"type": "usage", "data": { ...UsageRecord... }}
{"type": "savings", "data": { ...SavingsRecord... }}
```

The WS handler sends both types. On client connect:
1. Replay all `UsageRecord` history (existing behavior, now wrapped in envelope).
2. Replay all `SavingsRecord` history.
3. Stream live records of both types.

**Breaking change:** The WS message format changes. No external consumers exist beyond the bundled dashboard, so this is safe. The dashboard update happens in the same changeset.

### 4. Dashboard Changes

**`types/usage.ts` — new type:**
```typescript
interface SavingsRecord {
    timestamp: string;
    file: string;
    language: string;
    original_tokens: number;
    skeleton_tokens: number;
    saved_tokens: number;
}
```

**`useObsWebSocket.ts`:**
- Parse typed envelope, route to appropriate state array.
- Maintain `records: UsageRecord[]` (existing) and `savingsRecords: SavingsRecord[]` (new).
- Both clear on reconnect, both replay from server.

**New function `calculateSkltnSavings` in `types/usage.ts`:**
- Takes `savingsRecords: SavingsRecord[]` and `usageRecords: UsageRecord[]`.
- Returns `skltnSavings: number` (USD).
- Computation: `sum(saved_tokens) * dominant_model_input_rate / 1_000_000`.
- The dominant model is determined by which model has the highest total `input_tokens` across all usage records in the session (weighted by volume, not recency). Falls back to 0 if no usage records exist yet.
- **Known limitation:** The MCP server does not know which model the LLM client is using, so savings records carry no model field. The dollar figure is approximate — if the user switches models mid-session, savings are valued at the dominant model's rate rather than the rate active at the time of each skeletonization. In practice most sessions use a single model, so this is acceptably accurate.

**`MetricsBar.tsx`:**
- Add 5th metric: **SKLTN SAVINGS: $X.XX**.
- Same styling as existing CACHE SAVINGS metric.

## File Path Convention

Both `skltn-mcp` and `skltn-obs` resolve the savings file as:

```
dirs::home_dir() / ".skltn" / "savings.jsonl"
```

On macOS: `~/.skltn/savings.jsonl`
On Linux: `~/.skltn/savings.jsonl`

This matches the existing convention used by `skltn-obs` for `usage.jsonl`, where `default_data_dir()` resolves to `~/.skltn/`. Both crates must canonicalize the resolved path after directory creation per project security rules.

## Edge Cases

- **Obs starts before MCP:** Obs creates/truncates `savings.jsonl` on startup, then watches it. MCP appends when it starts.
- **MCP starts before obs:** Savings records accumulate in the file. When obs starts, it truncates the file (session reset) and only sees new records from that point.
- **Neither running:** File is inert. No impact.
- **Multiple MCP instances:** Unlikely but safe — append writes are atomic for lines under PIPE_BUF (4KB on most systems). SavingsRecords are well under this.
- **File rotation/cleanup:** Not needed for session-scoped data. File grows slowly (one line per skeletonized file, typically <200 bytes each). Can be manually deleted between sessions.

## Dependencies

- `notify` crate — already available in the Rust ecosystem, no new heavyweight dependencies. Needs to be added to `skltn-obs/Cargo.toml`.
- `dirs` crate — used by `skltn-obs` for path resolution. Must be added to `skltn-mcp/Cargo.toml` (not currently present).
- `serde` / `serde_json` — already used by both crates.
- `time` crate with features `["serde", "formatting", "parsing"]` — used by `skltn-obs`. Must be added to `skltn-mcp/Cargo.toml` (not currently present) for `SavingsRecord` timestamp serialization.

## What This Does NOT Track

- Token savings from files served full due to cache-awareness (these avoid re-skeletonization but don't reduce tokens vs the original — the full file was already sent).
- Output token differences (skeletonization only affects input context).
- Savings from the CLI tool (`skltn-cli`) — CLI is offline and doesn't write to this file.
