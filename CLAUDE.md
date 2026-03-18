# skltn

High-performance Rust toolchain for AI context window optimization via AST-based skeletonization and observability.

## Token Efficiency

Minimize token usage. Keep responses concise and direct. Prefer editing over rewriting files. Use parallel tool calls. Avoid restating context or repeating file contents unnecessarily.

Ignore these folders and files
.docs
.PRD
.PROGRESS
OPTIMISATIONS

## Rust Style

- Idiomatic Rust; follow clippy suggestions.
- No `unwrap()` in skltn-core; use `Result` with `thiserror`.
- `tokio` for async in skltn-mcp and skltn-obs.
- `time` crate, not `chrono`.
- `canonicalize()` + prefix checks for paths. Never leak absolute paths in errors.

## Commands

- Build: `cargo build --workspace`
- Test: `cargo test -p skltn-core`
- Lint: `cargo clippy --all-targets --all-features`
- CLI: `cargo run -p skltn-cli -- <PATH>`
- Proxy: `cargo run -p skltn-obs -- --port 8080`
