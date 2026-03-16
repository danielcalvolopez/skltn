🛠 Project Purpose
A high-performance Rust toolchain for AI context window optimization via AST-based skeletonization and observability.

📋 Standard Operating Procedures
Mandatory Logging: After completing any task (creating a spec/plan, finishing a phase, or a major code refactor), you MUST update the following tracking files to maintain project context:

MEMORY.md: Log the new "mental model" and any complex logic discovered or implemented.

PRD.md: Update phase status (e.g., from "Planned" to "In Progress" or "Complete").

project_status.md: Move sub-phases to "Complete" and update progress percentages.

project_key_decisions.md: Record any technical pivots or architectural "locks" made during the task.

Implementation Plans: Mark specific tasks as complete in the corresponding file within docs/superpowers/plans/.

PROGRESS.md: After EVERY task completion, blocker, or significant decision, update PROGRESS.md with the new status. This is the primary file for resuming work across conversations. Mark tasks as "Complete", "In Progress", or "Blocked" and add notes. Update the Session Log section at the bottom with a summary of what was done.

Context Window Management: At approximately 85% context usage, STOP current work immediately and prepare a handover. Update PROGRESS.md with: (1) exactly what was just completed, (2) what task/step is next, (3) any in-flight state or gotchas the next session needs to know. This ensures seamless continuation in a new conversation.

Rust Style Guide:

Use idiomatic Rust; follow clippy suggestions.

Avoid unwrap() in skltn-core; use Result with thiserror.

Use tokio for async logic in skltn-mcp and skltn-obs.

Use time crate (not chrono) for timestamps.

Security: Always use canonicalize() and prefix checks for path resolution. Never leak absolute paths in error messages.

🧪 Commands
Build All: cargo build --workspace

Test Core: cargo test -p skltn-core

Snapshot Tests: cargo test (insta snapshots)

Linting: cargo clippy --all-targets --all-features

CLI Run: cargo run -p skltn-cli -- <PATH>

Proxy Run: cargo run -p skltn-obs -- --port 8080
