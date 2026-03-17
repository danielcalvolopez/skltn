export PATH := env("HOME") / ".cargo/bin:" + env("PATH")

# Build the dashboard frontend
build-ui:
    cd crates/skltn-obs/dashboard && pnpm install && pnpm build

# Build everything (frontend + Rust)
build: build-ui
    cargo build --release -p skltn-obs

# Run the proxy in development mode
dev:
    cargo run -p skltn-obs -- --port 8080
