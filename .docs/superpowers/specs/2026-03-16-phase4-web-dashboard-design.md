# Phase 4: Web Dashboard — Design Specification

**Project:** skltn (Skeleton)
**Phase:** 4 of 4
**Date:** 2026-03-16
**Status:** Approved

---

## Overview

The Web Dashboard is a real-time monitoring interface served directly from the `skltn-obs` observability proxy. It consumes the existing WebSocket endpoint (`/ws`) to display session cost, cache savings, token usage, and per-request breakdowns in a TUI-inspired browser dashboard.

This is Phase 4 of the skltn project. It **modifies** the existing `skltn-obs` crate — there is no separate `skltn-hud` crate. The dashboard is a Vite + React + TypeScript application embedded into the `skltn-obs` binary via `rust-embed` at compile time.

> **PRD Deviation:** The original PRD specified a Tauri desktop app with floating window mode. This was changed to a web dashboard served from `skltn-obs`. Rationale: the proxy already runs an axum server, Tauri adds build complexity (separate binary, platform-specific packaging, native toolchain) for minimal benefit over a browser tab, and the entire Rust backend logic would be a trivial WebSocket consumer — not enough to justify Tauri's Rust-side architecture.

> **PRD Deviation:** The original "money saved" odometer was replaced with a "cache savings" odometer. Cache savings is a real, measurable metric: the difference between what cache-read tokens would have cost as regular input tokens vs what they cost at the discounted cache-read rate. This avoids speculative counterfactuals about "what the request would have looked like without skeletonization."

---

## Guiding Principles

1. **Consume, don't modify.** Phase 4 adds a dashboard to `skltn-obs`. It does not change the proxy, skimmer, tracker, or WebSocket logic from Phase 3. All Phase 3 modules remain untouched.
2. **Single binary.** The production build embeds all dashboard assets into the `skltn-obs` binary. No separate process, no Node.js runtime, no external files at deploy time.
3. **Data-dense, honest metrics.** Every number on the dashboard comes from real API responses. No estimates, no projections, no vanity metrics.

---

## Architecture

### Crate Changes

Phase 4 adds files to `skltn-obs`, not a new crate:

```
crates/skltn-obs/
├── Cargo.toml          # Add: rust-embed, mime_guess
├── src/
│   ├── main.rs         # Add: /dashboard route, / redirect
│   ├── dashboard.rs    # NEW: rust-embed static file handler
│   ├── proxy.rs        # Unchanged
│   ├── skim.rs         # Unchanged
│   ├── pricing.rs      # Unchanged
│   ├── tracker.rs      # Unchanged
│   └── ws.rs           # Unchanged
├── dashboard/          # NEW: Vite + React + TS project
│   ├── package.json
│   ├── pnpm-lock.yaml
│   ├── vite.config.ts
│   ├── tsconfig.json
│   ├── index.html
│   └── src/
│       ├── main.tsx
│       ├── App.tsx
│       ├── App.css
│       ├── assets/
│       │   └── JetBrainsMono/   # Bundled font files (woff2)
│       ├── hooks/
│       │   ├── useObsWebSocket.ts
│       │   ├── useSessionMetrics.ts
│       │   └── useChartData.ts
│       ├── components/
│       │   ├── MetricsBar.tsx
│       │   ├── TokenChart.tsx
│       │   ├── CacheRing.tsx
│       │   ├── ModelBreakdown.tsx
│       │   ├── RequestTable.tsx
│       │   └── ConnectionStatus.tsx
│       └── types/
│           └── usage.ts
```

### New Rust Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `rust-embed` | 8 | Embed `dashboard/dist/` into binary at compile time |
| `mime_guess` | 2 | Content-Type detection for static file serving |

### New JavaScript Dependencies

| Package | Purpose |
|---|---|
| `react`, `react-dom` | UI framework |
| `echarts`, `echarts-for-react` | Charting (time-series line chart) |
| `typescript` | Type safety |
| `vite`, `@vitejs/plugin-react` | Build toolchain |

No other JS dependencies. No state management library (Zustand, Redux). No CSS framework. Hand-written CSS in a single `App.css` file matching the TUI aesthetic. No component-level CSS files — the dashboard is small enough for one stylesheet.

### Build Tooling

A `justfile` at the workspace root orchestrates the build:

```just
build-ui:
    cd crates/skltn-obs/dashboard && pnpm install && pnpm build

build: build-ui
    cargo build --release -p skltn-obs

dev:
    cargo run -p skltn-obs -- --port 8080
```

---

## Routing

### Updated `skltn-obs` Router

```rust
let app = Router::new()
    .route("/", get(redirect_to_dashboard))
    .route("/ws", get(ws_handler))
    .nest_service("/dashboard", dashboard::static_handler())
    .fallback(proxy_handler)
    .with_state(state);
```

- `GET /` — 302 redirect to `/dashboard`. Convenience for users who open `localhost:8080` in a browser.
- `GET /ws` — WebSocket upgrade (Phase 3, unchanged).
- `GET /dashboard/**` — Serves embedded static assets (React app). `nest_service` strips the `/dashboard` prefix before passing to the handler.
- Everything else — Falls through to `proxy_handler` (Phase 3, unchanged).

### Why `/dashboard` Not `/`

The proxy's `fallback` handler catches all unmatched routes and forwards them to Anthropic. If the dashboard were served on `/`, it would shadow the proxy's catch-all for paths like `/assets/main.js`. Nesting under `/dashboard` avoids any risk of the React app's asset paths colliding with Anthropic API endpoints.

---

## Dashboard Handler (`dashboard.rs`)

```rust
use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "dashboard/dist/"]
struct Assets;

pub fn static_handler() -> axum::routing::MethodRouter {
    get(|uri: Uri| async move {
        let path = uri.path().trim_start_matches('/');
        let path = if path.is_empty() { "index.html" } else { path };

        match Assets::get(path) {
            Some(content) => {
                let mime = mime_guess::from_path(path).first_or_octet_stream();
                Response::builder()
                    .header(header::CONTENT_TYPE, mime.as_ref())
                    .body(Body::from(content.data))
                    .expect("valid MIME type produces valid header")
            }
            None => StatusCode::NOT_FOUND.into_response(),
        }
    })
}
```

`nest_service` strips the `/dashboard` prefix, so the handler receives paths relative to the dashboard root (e.g., `assets/main-abc123.js`, not `dashboard/assets/main-abc123.js`). For the root path (`/dashboard` or `/dashboard/`), the path is empty, so `index.html` is served.

No SPA routing fallback. This is a single-page dashboard — one `index.html`, one entry point. Unmatched asset paths return 404.

### Security Headers

All dashboard responses include the following headers:

- `Content-Security-Policy: default-src 'self'; connect-src 'self' ws://localhost:* ws://127.0.0.1:*; style-src 'self' 'unsafe-inline'; font-src 'self'` — prevents XSS via external script injection, restricts WebSocket connections to localhost
- `X-Content-Type-Options: nosniff` — prevents MIME-sniffing attacks
- `X-Frame-Options: DENY` — prevents clickjacking via iframe embedding

### Cache-Control

- `index.html`: `Cache-Control: no-cache` — ensures the browser always fetches the latest version after a proxy upgrade
- All other assets (Vite produces content-hashed filenames): `Cache-Control: public, max-age=31536000, immutable`

---

## Frontend Architecture

### Data Flow

```
WebSocket (/ws)
    → useObsWebSocket() — manages connection, exposes UsageRecord[], status
        → useSessionMetrics(records) — totals, cache ratio, per-model breakdown
        → useChartData(records) — ECharts series options
        → records passed directly to RequestTable
```

### Type Definition

```typescript
// types/usage.ts
interface UsageRecord {
    timestamp: string;          // RFC 3339
    model: string;              // e.g., "claude-opus-4-6-20260301"
    input_tokens: number;
    output_tokens: number;
    cache_creation_input_tokens: number;
    cache_read_input_tokens: number;
    cost_usd: number;
}
```

This mirrors the Rust `UsageRecord` struct from Phase 3, JSON-serialized over the WebSocket.

### Hooks

#### `useObsWebSocket()`

Manages the WebSocket connection lifecycle, reconnection, and the raw records array.

**Returns:**
- `records: UsageRecord[]` — all records received (replay + live), append-only
- `status: 'connecting' | 'open' | 'closed'` — current connection state

**Connection logic:**
- Connects to `ws://${window.location.host}/ws` (hardcoded `ws://` — the proxy runs on localhost over plain HTTP; `wss:` is not needed)
- On message: parse JSON as `UsageRecord`, append to records array via `setRecords(prev => [...prev, record])`
- On close: set status to `'closed'`, schedule reconnect with exponential backoff
- On open: set status to `'open'`, reset backoff to 1s, clear `records` array (replay will rebuild it)

**Reconnection:**
- Exponential backoff: 1s initial, 2x multiplier, 30s cap
- Backoff value stored in `useRef` (not state) — avoids re-renders on each reconnect attempt
- Reset to 1s on successful connection
- **On reconnect, clear the `records` array before receiving replay.** Phase 3's WebSocket sends a session replay on each new connection — the full `CostTracker.records` buffer. If the proxy was restarted, the replay is the new session's records. If the proxy was not restarted, the replay contains all prior records. Either way, the replay is the authoritative state. Clearing local records and rebuilding from replay avoids double-counting and keeps `useSessionMetrics` accurate.

**WebSocket URL:**
- Uses `window.location.host` to automatically work in both dev (Vite proxy on 5173) and production (Axum on 8080)
- Protocol: `ws:` for `http:`, `wss:` for `https:`

#### `useSessionMetrics(records)`

Derives aggregate metrics from the records array via `useMemo`.

**Returns:**
- `totalCost: number` — sum of all `cost_usd`
- `cacheSavings: number` — sum of per-record cache savings (see formula below)
- `requestCount: number` — `records.length`
- `totalTokens: number` — sum of `input_tokens + output_tokens`
- `cacheHitRatio: number` — `total_cache_read / total_input` (0-1), returns 0 when `total_input` is 0 (empty state)
- `modelBreakdown: { model: string, cost: number }[]` — sorted by cost descending

**Cache savings formula:**

```
per_record_savings = cache_read_input_tokens * (input_rate - cache_read_rate) / 1_000_000
```

Where `input_rate` and `cache_read_rate` are looked up from a client-side pricing table matching the `model` field. This is the same pricing data as Phase 3's `pricing.rs`, duplicated in the frontend as a simple lookup object.

The core computation is extracted as a pure function (`calculateSessionTotals`) for testability:

```typescript
export const calculateSessionTotals = (records: UsageRecord[]) => {
    return records.reduce((acc, record) => ({
        totalCost: acc.totalCost + record.cost_usd,
        totalTokens: acc.totalTokens + record.input_tokens + record.output_tokens,
        cacheSavings: acc.cacheSavings + calculateCacheSavings(record),
        totalCacheRead: acc.totalCacheRead + record.cache_read_input_tokens,
        totalInput: acc.totalInput + record.input_tokens,
    }), { totalCost: 0, totalTokens: 0, cacheSavings: 0, totalCacheRead: 0, totalInput: 0 });
};
```

#### `useChartData(records)`

Transforms records into ECharts option format via `useMemo`.

**Returns:** ECharts option object with:
- `xAxis: { type: 'time' }` — RFC 3339 timestamps on X axis
- Three series: input tokens, output tokens, cache read tokens
- Each series uses `step: 'end'` for sharp angles (brutalist aesthetic)
- `shadowBlur` on line styles for CRT glow effect

The memoization key is the `records` array reference. Since records are append-only and the array reference changes on each append, the chart options recompute on each new record. This is acceptable — ECharts diffing is efficient and the computation is trivial.

### Components

| Component | Props | Renders |
|---|---|---|
| `App` | (root) | Layout shell, owns `useObsWebSocket`, distributes data to children |
| `ConnectionStatus` | `status` | Top bar showing connection state. Hidden when `'open'`. Shows `// DISCONNECTED — reconnecting...` in dim red when `'closed'`. |
| `MetricsBar` | `totalCost`, `cacheSavings`, `requestCount`, `totalTokens` | 4 headline counters in a horizontal bar. Fixed-width decimals (`toFixed(2)` for USD, locale string for tokens). `font-variant-numeric: tabular-nums`. |
| `TokenChart` | `chartOptions` (from `useChartData`) | ECharts line chart. 3 series: input (bright green), output (mid green), cache read (dim green). Time-based X axis. Dark theme. |
| `CacheRing` | `ratio` (0-1) | SVG progress ring. Percentage label. Always green (accent color). |
| `ModelBreakdown` | `modelBreakdown` | List of models with cost and proportional bar. Sorted by cost descending. |
| `RequestTable` | `records` | 7-column table: timestamp, model, input, output, cache write, cache read, cost. Newest first. Sticky header. Zebra stripe `rgba(0,255,136,0.03)`. `overflow-y: auto` for scrolling. |

### Layout

```
┌─────────────────────────────────────────────────────────────────┐
│ [ConnectionStatus - hidden when connected]                       │
├────────────────────────────────────────────────────┬────────────┤
│ Session Cost │ Cache Savings │ Requests │ Tokens   │            │
│ $4.27        │ $1.83         │ 47       │ 2.1M     │            │
├────────────────────────────────────────────────────┤ Cache Ring │
│                                                    │   75%      │
│              Token Chart (ECharts)                  │            │
│              3 series, time-based X axis            │ ────────── │
│                                                    │ Model      │
│                                                    │ Breakdown  │
│                                                    │ opus: $3.12│
│                                                    │ son: $0.89 │
├────────────────────────────────────────────────────┴────────────┤
│ Request Table (7 columns, newest first, scrollable)             │
│ timestamp | model | input | output | cache_w | cache_r | cost   │
└─────────────────────────────────────────────────────────────────┘
```

Grid layout: `grid-template-columns: 1fr 200px`, `grid-template-rows: auto 1fr auto`.

---

## Visual Design

### Theme

- **Background:** `#0a0a0a` (obsidian)
- **Accent:** `#00ff88` (Matrix Green)
- **Accent mid:** `#008844` (secondary series)
- **Accent dim:** `#003322` (tertiary series, bars)
- **Text primary:** `#cccccc`
- **Text secondary:** `#666666`
- **Text muted:** `#444444`
- **Borders:** `#222222`, 1px solid
- **Panel background:** `#0d0d0d`

### Typography

- **Font:** JetBrains Mono (bundled in `dashboard/src/assets/` and loaded via `@font-face` — not from Google Fonts CDN, to ensure offline/localhost use works without external network dependencies)
- **Metric values:** 20px, weight 700, accent color
- **Labels:** 9px, uppercase, letter-spacing 1.5px, `#555555`
- **Table body:** 10px, monospace
- **All numbers:** `font-variant-numeric: tabular-nums` to prevent layout shift

### Aesthetic Rules

- No rounded corners anywhere. `border-radius: 0` on all elements.
- No gradients except `shadowBlur` on chart lines.
- No area fill on charts — thin lines only.
- Minimal padding — data-dense, functional layout.
- Sharp 1px borders. No shadows except chart glow.
- No emojis, no icons. Text and data only.

### ECharts Configuration

```typescript
{
    backgroundColor: 'transparent',
    grid: { top: 30, right: 12, bottom: 30, left: 50 },
    xAxis: { type: 'time', axisLine: { lineStyle: { color: '#222' } }, axisLabel: { color: '#444', fontFamily: 'JetBrains Mono', fontSize: 9 } },
    yAxis: { type: 'value', splitLine: { lineStyle: { color: '#1a1a1a' } }, axisLabel: { color: '#444', fontFamily: 'JetBrains Mono', fontSize: 9 } },
    series: [
        { name: 'Input', type: 'line', step: 'end', showSymbol: false, lineStyle: { width: 1.5, color: '#00ff88', shadowBlur: 6, shadowColor: 'rgba(0,255,136,0.3)' } },
        { name: 'Output', type: 'line', step: 'end', showSymbol: false, lineStyle: { width: 1.5, color: '#008844', shadowBlur: 4, shadowColor: 'rgba(0,136,68,0.2)' } },
        { name: 'Cache Read', type: 'line', step: 'end', showSymbol: false, lineStyle: { width: 1.5, color: '#003322', shadowBlur: 2, shadowColor: 'rgba(0,51,34,0.15)' } },
    ]
}
```

---

## Development Workflow

### Dev Mode

1. Terminal 1: `cargo run -p skltn-obs -- --port 8080` (starts the Rust proxy)
2. Terminal 2: `cd crates/skltn-obs/dashboard && pnpm dev` (starts Vite dev server on 5173)
3. Open `http://localhost:5173/dashboard/` in browser

Vite proxies `/ws` requests to `localhost:8080`:

```typescript
// dashboard/vite.config.ts
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
    plugins: [react()],
    base: '/dashboard/',
    server: {
        proxy: {
            '/ws': {
                target: 'http://localhost:8080',
                ws: true,
            },
        },
    },
});
```

### Production Build

1. `just build-ui` — runs `pnpm install && pnpm build` in `dashboard/`, outputs to `dashboard/dist/`
2. `just build` — runs `build-ui` then `cargo build --release -p skltn-obs`
3. `rust-embed` bakes `dashboard/dist/` into the binary at compile time
4. Single binary distribution: `./skltn-obs --port 8080`, open `localhost:8080/dashboard`

### Production WebSocket URL

In production, the browser loads the dashboard from `localhost:8080/dashboard`. The `useObsWebSocket` hook connects to `ws://localhost:8080/ws` using `window.location.host`. No configuration needed — same origin, same port.

---

## Client-Side Pricing Table

The frontend needs model pricing rates to calculate cache savings. This duplicates Phase 3's `pricing.rs` as a simple TypeScript object:

```typescript
// Duplicated from skltn-obs pricing.rs — update both when prices change
const MODEL_RATES: Record<string, { input: number; cacheRead: number }> = {
    'claude-opus-4': { input: 15.00, cacheRead: 1.50 },
    'claude-sonnet-4': { input: 3.00, cacheRead: 0.30 },
    'claude-haiku-4': { input: 0.80, cacheRead: 0.08 },
    'claude-3-7-sonnet': { input: 3.00, cacheRead: 0.30 },
    'claude-3-5-sonnet': { input: 3.00, cacheRead: 0.30 },
    'claude-3-5-haiku': { input: 0.80, cacheRead: 0.08 },
};

export const getCacheSavingsRate = (model: string): { input: number; cacheRead: number } => {
    const entry = Object.entries(MODEL_RATES).find(([key]) => model.includes(key));
    return entry ? entry[1] : { input: 0, cacheRead: 0 };
};

export const calculateCacheSavings = (record: UsageRecord): number => {
    const rates = getCacheSavingsRate(record.model);
    return (record.cache_read_input_tokens * (rates.input - rates.cacheRead)) / 1_000_000;
};
```

The `contains()`-style matching mirrors the Rust implementation — `model.includes(key)` handles date-stamped model IDs like `claude-sonnet-4-6-20260301`.

---

## Testing Strategy

### Rust Side

No dedicated tests for Phase 4's Rust changes. The only additions are:
- `dashboard.rs` — a trivial `rust-embed` static file server
- Routing changes in `main.rs` — adding two routes

These are verified by compilation (`rust-embed` fails to compile if `dashboard/dist/` is missing) and manual testing.

### Frontend

No unit tests for the initial build. The dashboard is a read-only consumer of WebSocket data with no user input, no forms, no complex interactions. If a component renders wrong, it's immediately visible.

**Manual verification checklist:**
1. Dev mode: Vite + Axum, dashboard loads, WebSocket connects, data flows
2. Production build: `just build`, dashboard loads at `/dashboard`, assets served correctly
3. Reconnection: kill and restart `skltn-obs`, verify the dashboard reconnects and replays
4. Multiple tabs: two browser tabs receive the same live records
5. Empty state: dashboard loads with no records, shows zero values, connects to WebSocket

**What warrants tests later:**
- If `calculateSessionTotals` or `calculateCacheSavings` gain complexity, unit test the pure functions
- If user interactions are added (filters, time range selection), test the interaction logic

---

## Success Criteria (Phase 4)

| Metric | Target |
|---|---|
| Dashboard load | Renders within 500ms of navigation to `/dashboard` |
| WebSocket connection | Connects within 1s of page load, replays session history |
| Reconnection | Recovers from proxy restart within 30s (backoff cap) |
| Data accuracy | Headline metrics match manual calculation from raw records |
| Asset embedding | Single `skltn-obs` binary serves dashboard without external files |
| Build pipeline | `just build` produces working binary with embedded dashboard |

---

## Out of Scope (Phase 4)

- Tauri desktop app / floating window mode (replaced by web dashboard)
- Speculative "savings from skeletonization" calculation
- Historical data analysis across sessions (JSONL file reading)
- User authentication on the dashboard
- Dark/light theme toggle (dark only)
- Mobile responsive design (desktop browser only)
- Export/download of session data
- Persistent dashboard settings or preferences
- Charts beyond the single time-series (no pie charts, no histograms)
- WebSocket message filtering or search
- Notifications or alerts
