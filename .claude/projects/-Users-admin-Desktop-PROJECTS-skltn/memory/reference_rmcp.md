---
name: rmcp crate API reference
description: Key API patterns for rmcp 1.2.0 Rust MCP server - struct setup, tool macros, stdio transport, parameter extraction, response types
type: reference
---

**Version:** rmcp 1.2.0 (pinned in Phase 2 plan)
**Features needed:** `server`, `transport-io`
**Also requires:** `schemars = "1.0"` (not 0.8)

**Server struct pattern:**
- Struct must be `Clone`, holds `ToolRouter<Self>` field
- `new()` goes in a separate `impl` block (not inside `#[tool_router]`)
- `#[tool_router]` on impl block containing `#[tool]` methods
- `#[tool_handler]` on `impl ServerHandler` block

**Tool parameters:** structs deriving `Deserialize + schemars::JsonSchema`, extracted via `Parameters<T>`

**Tool responses:** `CallToolResult::success(vec![Content::text("...")])` or `ErrorData::internal_error(msg, None)`

**Stdio transport:** `server.serve(stdio()).await?.waiting().await?` — logs MUST go to stderr

**Docs:** https://docs.rs/rmcp/1.2.0/rmcp/ | GitHub: modelcontextprotocol/rust-sdk
