# Phase 2: MCP Server Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust MCP server binary (`skltn-mcp`) that exposes Phase 1's Skeleton Engine over the Model Context Protocol with three tools: `list_repo_structure`, `read_skeleton`, and `read_full_symbol`.

**Architecture:** New `skltn-mcp` crate in the existing Cargo workspace. Stateless server using `rmcp` for MCP protocol over stdio transport. Budget Guard uses `tiktoken-rs` for real token counting (2k threshold). Symbol resolution walks tree-sitter ASTs with a scope stack for parent context. Path security via canonicalization. CPU-bound operations wrapped in `tokio::task::spawn_blocking`.

**Tech Stack:** Rust (latest stable), rmcp 1.2.0 (MCP server + stdio transport), tiktoken-rs, ignore, serde/serde_json, schemars 1.0, tokio

**Spec:** `docs/superpowers/specs/2026-03-16-phase2-mcp-server-design.md`

---

## File Structure

```
crates/skltn-mcp/
├── Cargo.toml
└── src/
    ├── main.rs                        # Server bootstrap, CLI arg parsing, stdio transport
    ├── tools/
    │   ├── mod.rs                     # Tool parameter structs, tool method implementations
    │   ├── list_repo_structure.rs     # Directory tree logic (ignore crate walking, formatting)
    │   ├── read_skeleton.rs           # Skeleton/full-file logic (budget guard integration)
    │   └── read_full_symbol.rs        # Symbol resolution invocation, response formatting
    ├── budget.rs                      # BudgetDecision enum, should_skeletonize(), token counting
    ├── resolve.rs                     # resolve_safe_path(), resolve_symbol(), ResolveResult, MatchInfo
    └── error.rs                       # McpError enum, conversions to content strings
```

**Responsibilities per file:**

| File | Responsibility |
|---|---|
| `main.rs` | Parse CLI args (repo root path), validate root, init tokenizer, init rmcp server, block on stdio transport |
| `tools/mod.rs` | `SkltnServer` struct (holds root + tokenizer), `#[tool_router]` impl with three `#[tool]` methods, `#[tool_handler]` impl for `ServerHandler` |
| `tools/list_repo_structure.rs` | `pub fn build_tree(root: &Path, relative: &str, max_depth: Option<usize>) -> String` — walks directory, formats tree output |
| `tools/read_skeleton.rs` | `pub fn read_skeleton_or_full(root: &Path, file: &str, tokenizer: &CoreBPE) -> String` — budget guard + skeletonization + response formatting |
| `tools/read_full_symbol.rs` | `pub fn read_full_symbol(root: &Path, file: &str, symbol: &str, start_line: Option<usize>, tokenizer: &CoreBPE) -> String` — resolve symbol + format response |
| `budget.rs` | `BudgetDecision` enum, `should_skeletonize()` function, `count_tokens()` helper |
| `resolve.rs` | `resolve_safe_path()`, `resolve_symbol()`, `ResolveResult`/`MatchInfo` types, data node tables, scope stack, doc comment/decorator look-back |
| `error.rs` | `McpError` enum, `Display` impl, `to_content_string()` for content responses |

---

## Chunk 1: Crate Scaffolding, Error Types, and Budget Guard

### Task 1: Add skltn-mcp Crate to Workspace

**Files:**
- Modify: `Cargo.toml` (workspace root — add `"crates/skltn-mcp"` to members)
- Create: `crates/skltn-mcp/Cargo.toml`
- Create: `crates/skltn-mcp/src/main.rs`

- [ ] **Step 1: Add skltn-mcp to workspace members**

In the workspace root `Cargo.toml`, add `"crates/skltn-mcp"` to the `members` list:

```toml
[workspace]
resolver = "2"
members = ["crates/skltn-core", "crates/skltn-cli", "crates/skltn-mcp"]
```

- [ ] **Step 2: Create skltn-mcp Cargo.toml**

`crates/skltn-mcp/Cargo.toml`:
```toml
[package]
name = "skltn-mcp"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "skltn-mcp"
path = "src/main.rs"

[dependencies]
skltn-core = { path = "../skltn-core" }
rmcp = { version = "1.2", features = ["server", "transport-io"] }
tiktoken-rs = "0.6"
tree-sitter = "0.24"
ignore = "0.4"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "1.0"
tokio = { version = "1", features = ["full"] }
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
insta = "1"
tempfile = "3"
```

- [ ] **Step 3: Create stub main.rs**

`crates/skltn-mcp/src/main.rs`:
```rust
fn main() {
    println!("skltn-mcp - not yet implemented");
}
```

- [ ] **Step 4: Verify workspace compiles**

Run: `cargo build`
Expected: Successful compilation. All three crates build.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/skltn-mcp/
git commit -m "chore: add skltn-mcp crate to workspace"
```

---

### Task 2: Define McpError Types

**Files:**
- Create: `crates/skltn-mcp/src/error.rs`
- Modify: `crates/skltn-mcp/src/main.rs` (add module declaration)

- [ ] **Step 1: Write error types**

`crates/skltn-mcp/src/error.rs`:
```rust
use std::fmt;

#[derive(Debug)]
pub enum McpError {
    InvalidRoot,
    FileNotFound(String),
    PathOutsideRoot,
    UnsupportedLanguage(String),
    SymbolNotFound { name: String, file: String },
    DirectoryNotFound(String),
    PathIsFile(String),
    NoSupportedFiles(String),
    Core(skltn_core::SkltnError),
}

impl fmt::Display for McpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            McpError::InvalidRoot => write!(f, "Invalid repository root path"),
            McpError::FileNotFound(path) => write!(f, "File not found: {path}"),
            McpError::PathOutsideRoot => write!(f, "Path is outside the repository root"),
            McpError::UnsupportedLanguage(path) => {
                write!(f, "Unsupported language for file: {path}. Supported: .rs, .py, .ts, .js")
            }
            McpError::SymbolNotFound { name, file } => {
                write!(f, "Symbol '{name}' not found in {file}")
            }
            McpError::DirectoryNotFound(path) => write!(f, "Directory not found: {path}"),
            McpError::PathIsFile(path) => {
                write!(f, "Path is a file, not a directory: {path}. Use read_skeleton to inspect it.")
            }
            McpError::NoSupportedFiles(path) => {
                write!(f, "No supported source files (.rs, .py, .ts, .js) found in {path}")
            }
            McpError::Core(e) => write!(f, "Engine error: {e}"),
        }
    }
}

impl From<skltn_core::SkltnError> for McpError {
    fn from(e: skltn_core::SkltnError) -> Self {
        McpError::Core(e)
    }
}
```

- [ ] **Step 2: Add module to main.rs**

Replace `crates/skltn-mcp/src/main.rs` with:
```rust
mod error;

fn main() {
    println!("skltn-mcp - not yet implemented");
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p skltn-mcp`
Expected: Successful compilation.

- [ ] **Step 4: Commit**

```bash
git add crates/skltn-mcp/src/error.rs crates/skltn-mcp/src/main.rs
git commit -m "feat(mcp): add McpError types for MCP-specific failure modes"
```

---

### Task 3: Implement Budget Guard

**Files:**
- Create: `crates/skltn-mcp/src/budget.rs`
- Create: `crates/skltn-mcp/tests/budget_test.rs`
- Modify: `crates/skltn-mcp/src/main.rs` (add module declaration)

- [ ] **Step 1: Write the failing test**

`crates/skltn-mcp/tests/budget_test.rs`:
```rust
use tiktoken_rs::CoreBPE;

// Helper to get the tokenizer — same one the server will use
fn tokenizer() -> CoreBPE {
    tiktoken_rs::cl100k_base().unwrap()
}

#[test]
fn test_small_file_returns_full() {
    let source = "fn main() {\n    println!(\"hello\");\n}\n";
    let tokenizer = tokenizer();
    let decision = skltn_mcp::budget::should_skeletonize(source, &tokenizer);
    match decision {
        skltn_mcp::budget::BudgetDecision::ReturnFull { original_tokens } => {
            assert!(original_tokens <= 2000);
            assert!(original_tokens > 0);
        }
        _ => panic!("Expected ReturnFull for small file"),
    }
}

#[test]
fn test_large_file_returns_skeletonize() {
    // Generate a source string that will exceed 2000 tokens
    let mut source = String::new();
    for i in 0..500 {
        source.push_str(&format!("fn function_{i}(arg: i32) -> i32 {{\n    arg + {i}\n}}\n\n"));
    }
    let tokenizer = tokenizer();
    let decision = skltn_mcp::budget::should_skeletonize(&source, &tokenizer);
    match decision {
        skltn_mcp::budget::BudgetDecision::Skeletonize { original_tokens } => {
            assert!(original_tokens > 2000);
        }
        _ => panic!("Expected Skeletonize for large file"),
    }
}

#[test]
fn test_count_tokens_returns_correct_count() {
    let tokenizer = tokenizer();
    let count = skltn_mcp::budget::count_tokens("hello world", &tokenizer);
    assert!(count > 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p skltn-mcp --test budget_test`
Expected: FAIL — `skltn_mcp::budget` module doesn't exist yet.

- [ ] **Step 3: Write the Budget Guard implementation**

`crates/skltn-mcp/src/budget.rs`:
```rust
use tiktoken_rs::CoreBPE;

const TOKEN_THRESHOLD: usize = 2_000;

#[derive(Debug)]
pub enum BudgetDecision {
    Skeletonize { original_tokens: usize },
    ReturnFull { original_tokens: usize },
}

pub fn should_skeletonize(source: &str, tokenizer: &CoreBPE) -> BudgetDecision {
    let token_count = tokenizer.encode_ordinary(source).len();
    if token_count > TOKEN_THRESHOLD {
        BudgetDecision::Skeletonize { original_tokens: token_count }
    } else {
        BudgetDecision::ReturnFull { original_tokens: token_count }
    }
}

pub fn count_tokens(text: &str, tokenizer: &CoreBPE) -> usize {
    tokenizer.encode_ordinary(text).len()
}
```

- [ ] **Step 4: Update main.rs to export budget module**

```rust
mod error;
pub mod budget;

fn main() {
    println!("skltn-mcp - not yet implemented");
}
```

Note: `budget` is `pub` so integration tests can access it.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p skltn-mcp --test budget_test`
Expected: All 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/skltn-mcp/src/budget.rs crates/skltn-mcp/src/main.rs crates/skltn-mcp/tests/budget_test.rs
git commit -m "feat(mcp): implement Budget Guard with tiktoken-rs token counting"
```

---

## Chunk 2: Path Security and Symbol Resolution

### Task 4: Implement Path Security

**Files:**
- Create: `crates/skltn-mcp/src/resolve.rs`
- Create: `crates/skltn-mcp/tests/path_security_test.rs`
- Modify: `crates/skltn-mcp/src/main.rs` (add module declaration)

- [ ] **Step 1: Write the failing tests**

`crates/skltn-mcp/tests/path_security_test.rs`:
```rust
use std::path::Path;
use std::fs;

#[test]
fn test_resolve_valid_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let file = root.join("test.rs");
    fs::write(&file, "fn main() {}").unwrap();

    let result = skltn_mcp::resolve::resolve_safe_path(root, "test.rs");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), file.canonicalize().unwrap());
}

#[test]
fn test_resolve_subdirectory_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let sub = root.join("src");
    fs::create_dir(&sub).unwrap();
    let file = sub.join("lib.rs");
    fs::write(&file, "pub fn hello() {}").unwrap();

    let result = skltn_mcp::resolve::resolve_safe_path(root, "src/lib.rs");
    assert!(result.is_ok());
}

#[test]
fn test_reject_path_traversal() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let result = skltn_mcp::resolve::resolve_safe_path(root, "../../../etc/passwd");
    assert!(result.is_err());
    match result.unwrap_err() {
        skltn_mcp::error::McpError::PathOutsideRoot => {}
        other => panic!("Expected PathOutsideRoot, got: {other:?}"),
    }
}

#[test]
fn test_reject_nonexistent_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let result = skltn_mcp::resolve::resolve_safe_path(root, "nonexistent.rs");
    assert!(result.is_err());
}

#[test]
fn test_resolve_dot_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let result = skltn_mcp::resolve::resolve_safe_path(root, ".");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), root.canonicalize().unwrap());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p skltn-mcp --test path_security_test`
Expected: FAIL — `skltn_mcp::resolve` module doesn't exist yet.

- [ ] **Step 4: Write the path security implementation**

`crates/skltn-mcp/src/resolve.rs`:
```rust
use std::path::{Path, PathBuf};
use crate::error::McpError;

pub fn resolve_safe_path(root: &Path, relative: &str) -> Result<PathBuf, McpError> {
    let joined = root.join(relative);
    let canonical_root = root.canonicalize().map_err(|_| McpError::InvalidRoot)?;
    let canonical_candidate = joined.canonicalize().map_err(|_| {
        McpError::FileNotFound(relative.to_string())
    })?;

    if !canonical_candidate.starts_with(&canonical_root) {
        return Err(McpError::PathOutsideRoot);
    }

    Ok(canonical_candidate)
}
```

- [ ] **Step 5: Update main.rs with module declaration**

```rust
pub mod error;
pub mod budget;
pub mod resolve;

fn main() {
    println!("skltn-mcp - not yet implemented");
}
```

Note: `error` is now `pub` too (tests need to match on `McpError` variants).

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p skltn-mcp --test path_security_test`
Expected: All 5 tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/skltn-mcp/src/resolve.rs crates/skltn-mcp/src/main.rs crates/skltn-mcp/Cargo.toml crates/skltn-mcp/tests/path_security_test.rs
git commit -m "feat(mcp): implement path security with canonicalize + prefix check"
```

---

### Task 5: Implement Symbol Resolution

**Files:**
- Modify: `crates/skltn-mcp/src/resolve.rs` (add symbol resolution types + algorithm)
- Create: `crates/skltn-mcp/tests/resolve_test.rs`

- [ ] **Step 1: Write the failing tests**

`crates/skltn-mcp/tests/resolve_test.rs`:
```rust
use skltn_core::backend::RustBackend;
use skltn_mcp::resolve::{resolve_symbol, ResolveResult};

#[test]
fn test_resolve_single_function() {
    let source = r#"
fn hello() {
    println!("hello");
}

fn world() {
    println!("world");
}
"#;
    let backend = RustBackend;
    let result = resolve_symbol(source, "hello", None, &backend);
    match result {
        ResolveResult::Found { match_info, .. } => {
            assert_eq!(match_info.name, "hello");
            assert!(match_info.parent_context.is_none());
        }
        other => panic!("Expected Found, got: {other:?}"),
    }
}

#[test]
fn test_resolve_method_in_impl() {
    let source = r#"
struct Foo;

impl Foo {
    fn bar(&self) {
        println!("bar");
    }
}
"#;
    let backend = RustBackend;
    let result = resolve_symbol(source, "bar", None, &backend);
    match result {
        ResolveResult::Found { match_info, .. } => {
            assert_eq!(match_info.name, "bar");
            assert_eq!(match_info.parent_context.as_deref(), Some("impl Foo"));
        }
        other => panic!("Expected Found, got: {other:?}"),
    }
}

#[test]
fn test_resolve_ambiguous_without_start_line() {
    let source = r#"
struct A;
struct B;

impl A {
    fn new() -> Self { A }
}

impl B {
    fn new() -> Self { B }
}
"#;
    let backend = RustBackend;
    let result = resolve_symbol(source, "new", None, &backend);
    match result {
        ResolveResult::Ambiguous { matches } => {
            assert_eq!(matches.len(), 2);
            assert_eq!(matches[0].parent_context.as_deref(), Some("impl A"));
            assert_eq!(matches[1].parent_context.as_deref(), Some("impl B"));
        }
        other => panic!("Expected Ambiguous, got: {other:?}"),
    }
}

#[test]
fn test_resolve_ambiguous_with_start_line() {
    let source = r#"
struct A;
struct B;

impl A {
    fn new() -> Self { A }
}

impl B {
    fn new() -> Self { B }
}
"#;
    let backend = RustBackend;
    // start_line close to the second impl's new()
    let result = resolve_symbol(source, "new", Some(10), &backend);
    match result {
        ResolveResult::Found { match_info, .. } => {
            assert_eq!(match_info.parent_context.as_deref(), Some("impl B"));
        }
        other => panic!("Expected Found with start_line disambiguation, got: {other:?}"),
    }
}

#[test]
fn test_resolve_not_found() {
    let source = "fn hello() {}\n";
    let backend = RustBackend;
    let result = resolve_symbol(source, "nonexistent", None, &backend);
    match result {
        ResolveResult::NotFound => {}
        other => panic!("Expected NotFound, got: {other:?}"),
    }
}

#[test]
fn test_resolve_struct_data_node() {
    let source = r#"
pub struct UserProfile {
    pub name: String,
    pub age: u32,
}
"#;
    let backend = RustBackend;
    let result = resolve_symbol(source, "UserProfile", None, &backend);
    match result {
        ResolveResult::Found { match_info, .. } => {
            assert_eq!(match_info.name, "UserProfile");
        }
        other => panic!("Expected Found for struct, got: {other:?}"),
    }
}

#[test]
fn test_resolve_enum_data_node() {
    let source = r#"
pub enum Color {
    Red,
    Green,
    Blue,
}
"#;
    let backend = RustBackend;
    let result = resolve_symbol(source, "Color", None, &backend);
    match result {
        ResolveResult::Found { match_info, .. } => {
            assert_eq!(match_info.name, "Color");
        }
        other => panic!("Expected Found for enum, got: {other:?}"),
    }
}

#[test]
fn test_resolve_lines_are_1_indexed() {
    let source = "fn hello() {\n    println!(\"hello\");\n}\n";
    let backend = RustBackend;
    let result = resolve_symbol(source, "hello", None, &backend);
    match result {
        ResolveResult::Found { match_info, .. } => {
            assert_eq!(match_info.start_line, 1, "Lines should be 1-indexed");
            assert!(match_info.end_line >= 1);
        }
        other => panic!("Expected Found, got: {other:?}"),
    }
}

#[test]
fn test_resolve_includes_doc_comments() {
    let source = r#"/// This function greets someone.
/// It returns a greeting string.
pub fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}
"#;
    let backend = RustBackend;
    let result = resolve_symbol(source, "greet", None, &backend);
    match result {
        ResolveResult::Found { source_text, match_info } => {
            // source_text should include the doc comments
            assert!(source_text.contains("/// This function greets someone."));
            assert!(source_text.contains("pub fn greet"));
            // start_line should point to the first doc comment, not the fn
            assert_eq!(match_info.start_line, 1);
        }
        other => panic!("Expected Found with doc comments, got: {other:?}"),
    }
}

#[test]
fn test_resolve_includes_attributes() {
    let source = r#"#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
}
"#;
    let backend = RustBackend;
    let result = resolve_symbol(source, "Config", None, &backend);
    match result {
        ResolveResult::Found { source_text, .. } => {
            assert!(source_text.contains("#[derive(Debug, Clone)]"));
            assert!(source_text.contains("pub struct Config"));
        }
        other => panic!("Expected Found with attributes, got: {other:?}"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p skltn-mcp --test resolve_test`
Expected: FAIL — `resolve_symbol`, `ResolveResult` don't exist yet.

- [ ] **Step 3: Write the symbol resolution implementation**

Add to `crates/skltn-mcp/src/resolve.rs` (below the existing `resolve_safe_path`):

```rust
use skltn_core::backend::LanguageBackend;
use tree_sitter::{Node, Parser};

#[derive(Debug)]
pub struct MatchInfo {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub parent_context: Option<String>,
}

#[derive(Debug)]
pub enum ResolveResult {
    Found {
        source_text: String,
        match_info: MatchInfo,
    },
    Ambiguous {
        matches: Vec<MatchInfo>,
    },
    NotFound,
}

/// Data node kinds that are valid lookup targets per language.
/// These are NOT structural nodes in Phase 1's sense (no body to prune),
/// but are valid symbols for `read_full_symbol`.
fn is_data_node(kind: &str, lang_extensions: &[&str]) -> bool {
    // Determine language from extensions
    let is_rust = lang_extensions.contains(&"rs");
    let is_typescript = lang_extensions.contains(&"ts");

    if is_rust {
        matches!(kind, "struct_item" | "enum_item" | "trait_item" | "type_item" | "const_item" | "static_item")
    } else if is_typescript {
        matches!(kind, "interface_declaration" | "type_alias_declaration" | "enum_declaration")
    } else {
        // Python and JavaScript have no additional data nodes
        // (classes and functions are already structural nodes)
        false
    }
}

/// Extract the name identifier from a node using the "name" field.
fn node_name<'a>(node: &Node<'a>, source: &'a [u8]) -> Option<String> {
    node.child_by_field_name("name")
        .map(|n| n.utf8_text(source).unwrap_or("").to_string())
}

/// Extract parent context string for container nodes (e.g., "impl Foo", "class Bar").
fn container_context(node: &Node, source: &[u8]) -> Option<String> {
    let kind = node.kind();
    match kind {
        "impl_item" => {
            // For Rust impl blocks: "impl TypeName" or "impl Trait for TypeName"
            let mut text = String::from("impl ");
            if let Some(trait_node) = node.child_by_field_name("trait") {
                let trait_name = trait_node.utf8_text(source).unwrap_or("");
                text.push_str(trait_name);
                text.push_str(" for ");
            }
            if let Some(type_node) = node.child_by_field_name("type") {
                let type_name = type_node.utf8_text(source).unwrap_or("");
                text.push_str(type_name);
            }
            Some(text)
        }
        "class_definition" | "class_declaration" => {
            node_name(node, source).map(|name| format!("class {name}"))
        }
        "module" => {
            node_name(node, source).map(|name| format!("mod {name}"))
        }
        _ => None,
    }
}

/// Look back at preceding siblings to find doc comments and decorators.
/// Returns the start byte of the earliest preceding doc/decorator sibling,
/// or the node's own start byte if none found.
fn extended_start_byte(node: &Node, source: &[u8], lang_extensions: &[&str]) -> usize {
    let is_python = lang_extensions.contains(&"py");
    let mut start = node.start_byte();
    let mut prev = node.prev_sibling();

    while let Some(sibling) = prev {
        let kind = sibling.kind();
        let is_doc_or_decorator = if is_python {
            kind == "decorator"
        } else {
            // Rust: attribute_item, line_comment (if starts with ///)
            // TS/JS: decorator
            kind == "attribute_item"
                || kind == "decorator"
                || (kind == "line_comment" && {
                    let text = sibling.utf8_text(source).unwrap_or("");
                    text.starts_with("///") || text.starts_with("//!")
                })
                || (kind == "block_comment" && {
                    let text = sibling.utf8_text(source).unwrap_or("");
                    text.starts_with("/**")
                })
        };

        if is_doc_or_decorator {
            start = sibling.start_byte();
            prev = sibling.prev_sibling();
        } else {
            break;
        }
    }

    start
}

pub fn resolve_symbol(
    source: &str,
    symbol: &str,
    start_line: Option<usize>,
    backend: &dyn LanguageBackend,
) -> ResolveResult {
    let mut parser = Parser::new();
    if parser.set_language(&backend.language()).is_err() {
        return ResolveResult::NotFound;
    }

    let tree = match parser.parse(source, None) {
        Some(tree) => tree,
        None => return ResolveResult::NotFound,
    };

    let source_bytes = source.as_bytes();
    let lang_extensions = backend.extensions();
    let mut matches: Vec<(MatchInfo, usize, usize)> = Vec::new(); // (info, extended_start_byte, end_byte)

    // Recursive depth-first walk with scope stack.
    // Using a recursive approach avoids the scope stack double-pop bug that
    // occurs with cursor-based iteration (where goto_parent is called once
    // per child, popping the scope multiple times for the same container).
    fn walk_node(
        node: tree_sitter::Node,
        source_bytes: &[u8],
        source: &str,
        symbol: &str,
        lang_extensions: &[&str],
        backend: &dyn LanguageBackend,
        scope_stack: &mut Vec<String>,
        matches: &mut Vec<(MatchInfo, usize, usize)>,
    ) {
        let kind = node.kind();
        let is_structural = backend.is_structural_node(&node);
        let is_data = is_data_node(kind, lang_extensions);

        // Push scope for container nodes (impl blocks, classes, modules).
        // We use container_context() to identify containers rather than
        // is_structural + body_node().is_none(), because containers like
        // impl blocks and classes DO have bodies in tree-sitter — body_node()
        // returns Some for them. container_context() directly checks node kinds.
        let pushed_scope = if let Some(ctx) = container_context(&node, source_bytes) {
            scope_stack.push(ctx);
            true
        } else {
            false
        };

        // Check for name match
        if is_structural || is_data {
            if let Some(name) = node_name(&node, source_bytes) {
                if name == symbol {
                    let ext_start = extended_start_byte(&node, source_bytes, lang_extensions);
                    let end = node.end_byte();
                    // Compute start_line from the extended range (includes doc comments/decorators)
                    let ext_start_line = source[..ext_start].matches('\n').count() + 1;
                    let info = MatchInfo {
                        name,
                        start_line: ext_start_line,
                        end_line: node.end_position().row + 1,
                        parent_context: scope_stack.last().cloned(),
                    };
                    matches.push((info, ext_start, end));
                }
            }
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk_node(child, source_bytes, source, symbol, lang_extensions, backend, scope_stack, matches);
        }

        // Pop scope when leaving this container node (exactly once)
        if pushed_scope {
            scope_stack.pop();
        }
    }

    let mut scope_stack: Vec<String> = Vec::new();
    walk_node(
        tree.root_node(),
        source_bytes,
        source,
        symbol,
        lang_extensions,
        backend,
        &mut scope_stack,
        &mut matches,
    );

    // Apply disambiguation
    match matches.len() {
        0 => ResolveResult::NotFound,
        1 => {
            let (info, ext_start, end) = matches.remove(0);
            let source_text = source[ext_start..end].to_string();
            ResolveResult::Found {
                source_text,
                match_info: info,
            }
        }
        _ => {
            if let Some(target_line) = start_line {
                // Compare using 1-indexed start_line values directly
                let closest_idx = matches
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, (info, _, _))| {
                        (info.start_line as isize - target_line as isize).unsigned_abs()
                    })
                    .map(|(idx, _)| idx)
                    .unwrap();

                let (info, ext_start, end) = matches.remove(closest_idx);
                let source_text = source[ext_start..end].to_string();
                ResolveResult::Found {
                    source_text,
                    match_info: info,
                }
            } else {
                let match_infos = matches.into_iter().map(|(info, _, _)| info).collect();
                ResolveResult::Ambiguous { matches: match_infos }
            }
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p skltn-mcp --test resolve_test`
Expected: All 8 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/skltn-mcp/src/resolve.rs crates/skltn-mcp/tests/resolve_test.rs
git commit -m "feat(mcp): implement symbol resolution with scope stack and data node support"
```

---

### Task 6: Add TypeScript Symbol Resolution Tests

**Files:**
- Modify: `crates/skltn-mcp/tests/resolve_test.rs` (add TS tests)

- [ ] **Step 1: Write TypeScript resolution tests**

Append to `crates/skltn-mcp/tests/resolve_test.rs`:
```rust
use skltn_core::backend::TypeScriptBackend;

#[test]
fn test_resolve_ts_interface() {
    let source = r#"
interface UserProfile {
    name: string;
    age: number;
}
"#;
    let backend = TypeScriptBackend;
    let result = resolve_symbol(source, "UserProfile", None, &backend);
    match result {
        ResolveResult::Found { match_info, .. } => {
            assert_eq!(match_info.name, "UserProfile");
        }
        other => panic!("Expected Found for TS interface, got: {other:?}"),
    }
}

#[test]
fn test_resolve_ts_type_alias() {
    let source = r#"
type Color = "red" | "green" | "blue";
"#;
    let backend = TypeScriptBackend;
    let result = resolve_symbol(source, "Color", None, &backend);
    match result {
        ResolveResult::Found { match_info, .. } => {
            assert_eq!(match_info.name, "Color");
        }
        other => panic!("Expected Found for TS type alias, got: {other:?}"),
    }
}

#[test]
fn test_resolve_ts_enum() {
    let source = r#"
enum Direction {
    Up,
    Down,
    Left,
    Right,
}
"#;
    let backend = TypeScriptBackend;
    let result = resolve_symbol(source, "Direction", None, &backend);
    match result {
        ResolveResult::Found { match_info, .. } => {
            assert_eq!(match_info.name, "Direction");
        }
        other => panic!("Expected Found for TS enum, got: {other:?}"),
    }
}

#[test]
fn test_resolve_ts_method_in_class() {
    let source = r#"
class UserService {
    getUser(id: string): User {
        return db.find(id);
    }
}
"#;
    let backend = TypeScriptBackend;
    let result = resolve_symbol(source, "getUser", None, &backend);
    match result {
        ResolveResult::Found { match_info, .. } => {
            assert_eq!(match_info.name, "getUser");
            assert_eq!(match_info.parent_context.as_deref(), Some("class UserService"));
        }
        other => panic!("Expected Found, got: {other:?}"),
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p skltn-mcp --test resolve_test`
Expected: All 12 tests pass (8 Rust + 4 TypeScript).

- [ ] **Step 3: Commit**

```bash
git add crates/skltn-mcp/tests/resolve_test.rs
git commit -m "test(mcp): add TypeScript symbol resolution tests for interfaces, type aliases, enums"
```

---

## Chunk 3: Tool Implementations (list_repo_structure and read_skeleton)

### Task 7: Implement list_repo_structure Logic

**Files:**
- Create: `crates/skltn-mcp/src/tools/mod.rs`
- Create: `crates/skltn-mcp/src/tools/list_repo_structure.rs`
- Create: `crates/skltn-mcp/tests/list_repo_structure_test.rs`
- Modify: `crates/skltn-mcp/src/main.rs` (add module declaration)

- [ ] **Step 1: Write the failing tests**

`crates/skltn-mcp/tests/list_repo_structure_test.rs`:
```rust
use std::fs;
use std::path::Path;

fn create_test_repo(root: &Path) {
    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("main.rs"), "fn main() {}").unwrap();
    fs::write(src.join("lib.rs"), "pub fn hello() {}").unwrap();

    let backend = src.join("backend");
    fs::create_dir_all(&backend).unwrap();
    fs::write(backend.join("mod.rs"), "pub mod rust;").unwrap();
    fs::write(backend.join("rust.rs"), "pub struct RustBackend;").unwrap();

    // Unsupported file — should be omitted
    fs::write(src.join("README.md"), "# Hello").unwrap();

    let tests = root.join("tests");
    fs::create_dir_all(&tests).unwrap();
    fs::write(tests.join("integration.rs"), "#[test] fn it_works() {}").unwrap();
}

#[test]
fn test_basic_tree_output() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    let output = skltn_mcp::tools::list_repo_structure::build_tree(dir.path(), ".", None);
    assert!(output.contains("src/"));
    assert!(output.contains("main.rs"));
    assert!(output.contains("lib.rs"));
    assert!(output.contains("backend/"));
    assert!(output.contains("rust.rs"));
    assert!(output.contains("tests/"));
    assert!(output.contains("integration.rs"));
    // Unsupported files omitted
    assert!(!output.contains("README.md"));
    // File metadata present
    assert!(output.contains("bytes"));
    assert!(output.contains("rust"));
}

#[test]
fn test_max_depth_limits_traversal() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    let output = skltn_mcp::tools::list_repo_structure::build_tree(dir.path(), ".", Some(1));
    // Depth 1: src/ and tests/ visible, but not src/backend/
    assert!(output.contains("src/"));
    assert!(output.contains("main.rs"));
    assert!(!output.contains("backend/"));
}

#[test]
fn test_subdirectory_listing() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    let output = skltn_mcp::tools::list_repo_structure::build_tree(dir.path(), "src", None);
    assert!(output.contains("main.rs"));
    assert!(output.contains("backend/"));
    // Should NOT show tests/ (we're listing src/ only)
    assert!(!output.contains("integration.rs"));
}

#[test]
fn test_empty_directory_pruned() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let empty = root.join("empty_dir");
    fs::create_dir_all(&empty).unwrap();
    // Also create a dir with only unsupported files
    let docs = root.join("docs");
    fs::create_dir_all(&docs).unwrap();
    fs::write(docs.join("notes.md"), "# Notes").unwrap();
    // One valid file at root
    fs::write(root.join("main.rs"), "fn main() {}").unwrap();

    let output = skltn_mcp::tools::list_repo_structure::build_tree(root, ".", None);
    assert!(!output.contains("empty_dir"));
    assert!(!output.contains("docs"));
    assert!(output.contains("main.rs"));
}

#[test]
fn test_python_files_detected() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("app.py"), "def main(): pass").unwrap();

    let output = skltn_mcp::tools::list_repo_structure::build_tree(root, ".", None);
    assert!(output.contains("app.py"));
    assert!(output.contains("python"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p skltn-mcp --test list_repo_structure_test`
Expected: FAIL — module doesn't exist yet.

- [ ] **Step 3: Write the implementation**

`crates/skltn-mcp/src/tools/list_repo_structure.rs`:
```rust
use std::collections::BTreeMap;
use std::path::Path;
use super::language_name;

/// Returns the language name for an extension if it's a supported source file.
/// Uses the shared language_name() from tools/mod.rs.
fn language_for_extension(ext: &str) -> Option<&'static str> {
    let name = language_name(ext);
    if name == "unknown" { None } else { Some(name) }
}

/// Represents a directory tree node.
enum TreeNode {
    File { size: u64, language: String },
    Dir { children: BTreeMap<String, TreeNode> },
}

impl TreeNode {
    fn has_supported_files(&self) -> bool {
        match self {
            TreeNode::File { .. } => true,
            TreeNode::Dir { children } => children.values().any(|c| c.has_supported_files()),
        }
    }
}

pub fn build_tree(root: &Path, relative: &str, max_depth: Option<usize>) -> String {
    let target = if relative == "." {
        root.to_path_buf()
    } else {
        root.join(relative)
    };

    // Build the tree structure by walking with the ignore crate
    let mut tree_root: BTreeMap<String, TreeNode> = BTreeMap::new();

    let walker = ignore::WalkBuilder::new(&target)
        .hidden(true)
        .git_ignore(true)
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if path == target {
            continue;
        }

        // Get path relative to target
        let rel = match path.strip_prefix(&target) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let components: Vec<&str> = rel.components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect();

        if components.is_empty() {
            continue;
        }

        if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let lang = match language_for_extension(ext) {
                Some(l) => l,
                None => continue, // Skip unsupported files
            };

            // Check depth
            if let Some(max) = max_depth {
                if components.len() > max + 1 {
                    continue;
                }
            }

            let size = path.metadata().map(|m| m.len()).unwrap_or(0);

            // Insert into tree
            let mut current = &mut tree_root;
            for (i, component) in components.iter().enumerate() {
                if i == components.len() - 1 {
                    current.insert(
                        component.to_string(),
                        TreeNode::File { size, language: lang.to_string() },
                    );
                } else {
                    let entry = current
                        .entry(component.to_string())
                        .or_insert_with(|| TreeNode::Dir { children: BTreeMap::new() });
                    if let TreeNode::Dir { children } = entry {
                        current = children;
                    } else {
                        break;
                    }
                }
            }
        }
    }

    // Render tree to string
    let mut output = String::new();
    render_tree(&tree_root, &mut output, 0);
    output
}

fn render_tree(nodes: &BTreeMap<String, TreeNode>, output: &mut String, depth: usize) {
    let indent = "  ".repeat(depth);
    for (name, node) in nodes {
        match node {
            TreeNode::File { size, language } => {
                let formatted_size = format_bytes(*size);
                output.push_str(&format!("{indent}{name} ({formatted_size}, {language})\n"));
            }
            TreeNode::Dir { children } => {
                if !node.has_supported_files() {
                    continue; // Prune empty directories
                }
                output.push_str(&format!("{indent}{name}/\n"));
                render_tree(children, output, depth + 1);
            }
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    // Format with comma separators to match spec (e.g., "4,821 bytes")
    let s = bytes.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    let formatted: String = result.chars().rev().collect();
    format!("{formatted} bytes")
}
```

`crates/skltn-mcp/src/tools/mod.rs`:
```rust
pub mod list_repo_structure;

use skltn_core::backend::{
    LanguageBackend, RustBackend, PythonBackend, TypeScriptBackend, JavaScriptBackend,
};

pub fn backend_for_extension(ext: &str) -> Option<Box<dyn LanguageBackend>> {
    match ext {
        "rs" => Some(Box::new(RustBackend)),
        "py" => Some(Box::new(PythonBackend)),
        "ts" => Some(Box::new(TypeScriptBackend)),
        "js" => Some(Box::new(JavaScriptBackend)),
        _ => None,
    }
}

pub fn language_name(ext: &str) -> &'static str {
    match ext {
        "rs" => "rust",
        "py" => "python",
        "ts" => "typescript",
        "js" => "javascript",
        _ => "unknown",
    }
}

pub fn has_parse_errors(source: &str, backend: &dyn LanguageBackend) -> bool {
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&backend.language()).is_err() {
        return true;
    }
    match parser.parse(source, None) {
        Some(tree) => has_error_nodes(tree.root_node()),
        None => true,
    }
}

fn has_error_nodes(node: tree_sitter::Node) -> bool {
    if node.is_error() || node.is_missing() {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_error_nodes(child) {
            return true;
        }
    }
    false
}
```

- [ ] **Step 4: Update main.rs with tools module**

```rust
pub mod error;
pub mod budget;
pub mod resolve;
pub mod tools;

fn main() {
    println!("skltn-mcp - not yet implemented");
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p skltn-mcp --test list_repo_structure_test`
Expected: All 5 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/skltn-mcp/src/tools/ crates/skltn-mcp/src/main.rs crates/skltn-mcp/tests/list_repo_structure_test.rs
git commit -m "feat(mcp): implement list_repo_structure directory tree builder"
```

---

### Task 8: Implement read_skeleton Logic

**Files:**
- Create: `crates/skltn-mcp/src/tools/read_skeleton.rs`
- Create: `crates/skltn-mcp/tests/read_skeleton_test.rs`
- Modify: `crates/skltn-mcp/src/tools/mod.rs`

- [ ] **Step 1: Write the failing tests**

`crates/skltn-mcp/tests/read_skeleton_test.rs`:
```rust
use std::fs;

fn tokenizer() -> tiktoken_rs::CoreBPE {
    tiktoken_rs::cl100k_base().unwrap()
}

#[test]
fn test_small_file_returned_full() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let source = "fn main() {\n    println!(\"hello\");\n}\n";
    fs::write(root.join("main.rs"), source).unwrap();

    let tok = tokenizer();
    let output = skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "main.rs", &tok);

    assert!(output.contains("[file: main.rs"));
    assert!(output.contains("full file"));
    assert!(output.contains("fn main()"));
}

#[test]
fn test_large_file_returned_skeletonized() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Generate a large Rust file
    let mut source = String::new();
    for i in 0..200 {
        source.push_str(&format!(
            "pub fn function_{i}(x: i32) -> i32 {{\n    let a = x + 1;\n    let b = a * 2;\n    let c = b - 3;\n    c + {i}\n}}\n\n"
        ));
    }
    fs::write(root.join("big.rs"), &source).unwrap();

    let tok = tokenizer();
    let output = skltn_mcp::tools::read_skeleton::read_skeleton_or_full(root, "big.rs", &tok);

    assert!(output.contains("[file: big.rs"));
    assert!(output.contains("skeleton:"));
    assert!(output.contains("compression:"));
    assert!(output.contains("todo!()"));
}

#[test]
fn test_file_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let tok = tokenizer();
    let output = skltn_mcp::tools::read_skeleton::read_skeleton_or_full(dir.path(), "nope.rs", &tok);
    assert!(output.contains("File not found: nope.rs"));
}

#[test]
fn test_unsupported_language() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("readme.md"), "# Hello").unwrap();
    let tok = tokenizer();
    let output = skltn_mcp::tools::read_skeleton::read_skeleton_or_full(dir.path(), "readme.md", &tok);
    assert!(output.contains("Unsupported language"));
}

#[test]
fn test_path_traversal_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let tok = tokenizer();
    let output = skltn_mcp::tools::read_skeleton::read_skeleton_or_full(dir.path(), "../../../etc/passwd", &tok);
    // Should get an error, not file contents
    assert!(
        output.contains("Path is outside the repository root")
        || output.contains("File not found")
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p skltn-mcp --test read_skeleton_test`
Expected: FAIL — module doesn't exist yet.

- [ ] **Step 3: Write the implementation**

`crates/skltn-mcp/src/tools/read_skeleton.rs`:
```rust
use std::path::Path;
use tiktoken_rs::CoreBPE;
use skltn_core::engine::SkeletonEngine;
use skltn_core::options::SkeletonOptions;
use crate::budget::{self, BudgetDecision};
use crate::error::McpError;
use crate::resolve::resolve_safe_path;
use super::{backend_for_extension, language_name, has_parse_errors};

pub fn read_skeleton_or_full(root: &Path, file: &str, tokenizer: &CoreBPE) -> String {
    // Resolve path
    let path = match resolve_safe_path(root, file) {
        Ok(p) => p,
        Err(e) => return e.to_string(),
    };

    // Check it's a file
    if !path.is_file() {
        return McpError::FileNotFound(file.to_string()).to_string();
    }

    // Detect language
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let backend = match backend_for_extension(ext) {
        Some(b) => b,
        None => return McpError::UnsupportedLanguage(file.to_string()).to_string(),
    };

    // Read file
    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return McpError::FileNotFound(file.to_string()).to_string(),
    };

    let lang = language_name(ext);

    // Budget Guard decision
    match budget::should_skeletonize(&source, tokenizer) {
        BudgetDecision::ReturnFull { original_tokens } => {
            let warning = if has_parse_errors(&source, backend.as_ref()) {
                " | warning: parse errors detected"
            } else {
                ""
            };
            format!(
                "[file: {file} | language: {lang} | tokens: {original_tokens} | full file{warning}]\n\n{source}"
            )
        }
        BudgetDecision::Skeletonize { original_tokens } => {
            let opts = SkeletonOptions::default();
            let skeleton = match SkeletonEngine::skeletonize(&source, backend.as_ref(), &opts) {
                Ok(s) => s,
                Err(e) => return format!("Engine error: {e}"),
            };

            let skeleton_tokens = budget::count_tokens(&skeleton, tokenizer);
            let compression = if original_tokens > 0 {
                ((1.0 - skeleton_tokens as f64 / original_tokens as f64) * 100.0) as u32
            } else {
                0
            };

            let warning = if has_parse_errors(&source, backend.as_ref()) {
                " | warning: parse errors detected"
            } else {
                ""
            };

            format!(
                "[file: {file} | language: {lang} | original: {original_tokens} tokens | skeleton: {skeleton_tokens} tokens | compression: {compression}%{warning}]\n\n{skeleton}"
            )
        }
    }
}
```

- [ ] **Step 4: Update tools/mod.rs**

```rust
pub mod list_repo_structure;
pub mod read_skeleton;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p skltn-mcp --test read_skeleton_test`
Expected: All 5 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/skltn-mcp/src/tools/read_skeleton.rs crates/skltn-mcp/src/tools/mod.rs crates/skltn-mcp/tests/read_skeleton_test.rs
git commit -m "feat(mcp): implement read_skeleton with Budget Guard integration"
```

---

## Chunk 4: Tool Implementation (read_full_symbol) and Response Formatting

### Task 9: Implement read_full_symbol Logic

**Files:**
- Create: `crates/skltn-mcp/src/tools/read_full_symbol.rs`
- Create: `crates/skltn-mcp/tests/read_full_symbol_test.rs`
- Modify: `crates/skltn-mcp/src/tools/mod.rs`

- [ ] **Step 1: Write the failing tests**

`crates/skltn-mcp/tests/read_full_symbol_test.rs`:
```rust
use std::fs;

fn tokenizer() -> tiktoken_rs::CoreBPE {
    tiktoken_rs::cl100k_base().unwrap()
}

#[test]
fn test_single_match_returns_full_source() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let source = r#"pub fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}

pub fn farewell() -> &'static str {
    "Goodbye!"
}
"#;
    fs::write(root.join("lib.rs"), source).unwrap();

    let tok = tokenizer();
    let output = skltn_mcp::tools::read_full_symbol::read_full_symbol(root, "lib.rs", "greet", None, &tok);
    assert!(output.contains("[symbol: greet"));
    assert!(output.contains("pub fn greet(name: &str) -> String"));
    assert!(output.contains("format!(\"Hello, {name}!\")"));
}

#[test]
fn test_ambiguous_returns_list() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let source = r#"struct A;
struct B;

impl A {
    pub fn new() -> Self { A }
}

impl B {
    pub fn new() -> Self { B }
}
"#;
    fs::write(root.join("lib.rs"), source).unwrap();

    let tok = tokenizer();
    let output = skltn_mcp::tools::read_full_symbol::read_full_symbol(root, "lib.rs", "new", None, &tok);
    assert!(output.contains("Multiple matches for 'new'"));
    assert!(output.contains("impl A"));
    assert!(output.contains("impl B"));
    assert!(output.contains("start_line"));
}

#[test]
fn test_start_line_disambiguates() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let source = r#"struct A;
struct B;

impl A {
    pub fn new() -> Self { A }
}

impl B {
    pub fn new() -> Self { B }
}
"#;
    fs::write(root.join("lib.rs"), source).unwrap();

    let tok = tokenizer();
    let output = skltn_mcp::tools::read_full_symbol::read_full_symbol(root, "lib.rs", "new", Some(9), &tok);
    assert!(output.contains("[symbol: new"));
    assert!(output.contains("impl B"));
}

#[test]
fn test_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("lib.rs"), "fn hello() {}").unwrap();

    let tok = tokenizer();
    let output = skltn_mcp::tools::read_full_symbol::read_full_symbol(root, "lib.rs", "nonexistent", None, &tok);
    assert!(output.contains("Symbol 'nonexistent' not found in lib.rs"));
}

#[test]
fn test_file_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let tok = tokenizer();
    let output = skltn_mcp::tools::read_full_symbol::read_full_symbol(dir.path(), "nope.rs", "foo", None, &tok);
    assert!(output.contains("File not found: nope.rs"));
}

#[test]
fn test_data_node_struct_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let source = r#"pub struct Config {
    pub host: String,
    pub port: u16,
}
"#;
    fs::write(root.join("config.rs"), source).unwrap();

    let tok = tokenizer();
    let output = skltn_mcp::tools::read_full_symbol::read_full_symbol(root, "config.rs", "Config", None, &tok);
    assert!(output.contains("[symbol: Config"));
    assert!(output.contains("pub host: String"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p skltn-mcp --test read_full_symbol_test`
Expected: FAIL — module doesn't exist yet.

- [ ] **Step 3: Write the implementation**

`crates/skltn-mcp/src/tools/read_full_symbol.rs`:
```rust
use std::path::Path;
use tiktoken_rs::CoreBPE;
use crate::budget;
use crate::error::McpError;
use crate::resolve::{resolve_safe_path, resolve_symbol, ResolveResult};
use super::{backend_for_extension, language_name, has_parse_errors};

pub fn read_full_symbol(
    root: &Path,
    file: &str,
    symbol: &str,
    start_line: Option<usize>,
    tokenizer: &CoreBPE,
) -> String {
    // Resolve path
    let path = match resolve_safe_path(root, file) {
        Ok(p) => p,
        Err(e) => return e.to_string(),
    };

    if !path.is_file() {
        return McpError::FileNotFound(file.to_string()).to_string();
    }

    // Detect language
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let backend = match backend_for_extension(ext) {
        Some(b) => b,
        None => return McpError::UnsupportedLanguage(file.to_string()).to_string(),
    };

    // Read file
    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return McpError::FileNotFound(file.to_string()).to_string(),
    };

    let lang = language_name(ext);
    let parse_warning = if has_parse_errors(&source, backend.as_ref()) {
        " | warning: parse errors detected"
    } else {
        ""
    };

    // Resolve symbol
    match resolve_symbol(&source, symbol, start_line, backend.as_ref()) {
        ResolveResult::Found { source_text, match_info } => {
            let tokens = budget::count_tokens(&source_text, tokenizer);
            format!(
                "[symbol: {} | file: {} | lines: {}-{} | {} tokens{}]\n\n{}",
                match_info.name,
                file,
                match_info.start_line,
                match_info.end_line,
                tokens,
                parse_warning,
                source_text,
            )
        }
        ResolveResult::Ambiguous { matches } => {
            let mut result = format!("Multiple matches for '{symbol}':\n");
            for m in &matches {
                let context = m.parent_context.as_deref().unwrap_or("top-level");
                result.push_str(&format!(
                    "  - {} (lines {}-{}) in {}\n",
                    m.name, m.start_line, m.end_line, context,
                ));
            }
            result.push_str("\nPlease re-call with start_line to select one.");
            result
        }
        ResolveResult::NotFound => {
            McpError::SymbolNotFound {
                name: symbol.to_string(),
                file: file.to_string(),
            }.to_string()
        }
    }
}
```

- [ ] **Step 4: Update tools/mod.rs**

```rust
pub mod list_repo_structure;
pub mod read_skeleton;
pub mod read_full_symbol;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p skltn-mcp --test read_full_symbol_test`
Expected: All 6 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/skltn-mcp/src/tools/read_full_symbol.rs crates/skltn-mcp/src/tools/mod.rs crates/skltn-mcp/tests/read_full_symbol_test.rs
git commit -m "feat(mcp): implement read_full_symbol with symbol resolution and disambiguation"
```

---

## Chunk 5: MCP Server Wiring (rmcp Integration)

### Task 10: Wire Up SkltnServer with rmcp Tool Registration

**Files:**
- Modify: `crates/skltn-mcp/src/tools/mod.rs` (add SkltnServer struct, tool impls, ServerHandler)
- Modify: `crates/skltn-mcp/src/main.rs` (server bootstrap)

- [ ] **Step 1: Write the SkltnServer struct and tool handlers**

Replace `crates/skltn-mcp/src/tools/mod.rs` with the full version that retains the shared utilities from Task 7 and adds the SkltnServer struct, tool handlers, and ServerHandler:
```rust
pub mod list_repo_structure;
pub mod read_skeleton;
pub mod read_full_symbol;

use std::path::PathBuf;
use std::sync::Arc;
use rmcp::{
    ErrorData, ServerHandler, schemars, tool, tool_handler, tool_router,
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::*,
};
use serde::Deserialize;
use tiktoken_rs::CoreBPE;

use skltn_core::backend::{
    LanguageBackend, RustBackend, PythonBackend, TypeScriptBackend, JavaScriptBackend,
};
use crate::error::McpError;
use crate::resolve::resolve_safe_path;

// --- Shared utility functions (used by read_skeleton and read_full_symbol) ---

pub fn backend_for_extension(ext: &str) -> Option<Box<dyn LanguageBackend>> {
    match ext {
        "rs" => Some(Box::new(RustBackend)),
        "py" => Some(Box::new(PythonBackend)),
        "ts" => Some(Box::new(TypeScriptBackend)),
        "js" => Some(Box::new(JavaScriptBackend)),
        _ => None,
    }
}

pub fn language_name(ext: &str) -> &'static str {
    match ext {
        "rs" => "rust",
        "py" => "python",
        "ts" => "typescript",
        "js" => "javascript",
        _ => "unknown",
    }
}

pub fn has_parse_errors(source: &str, backend: &dyn LanguageBackend) -> bool {
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&backend.language()).is_err() {
        return true;
    }
    match parser.parse(source, None) {
        Some(tree) => has_error_nodes(tree.root_node()),
        None => true,
    }
}

fn has_error_nodes(node: tree_sitter::Node) -> bool {
    if node.is_error() || node.is_missing() {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_error_nodes(child) {
            return true;
        }
    }
    false
}

// --- Parameter structs ---

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListRepoStructureParams {
    /// Subdirectory to list, relative to repo root. Defaults to "." (repo root).
    #[serde(default = "default_path")]
    pub path: String,

    /// Maximum directory depth to traverse. Omit for unlimited.
    #[serde(default)]
    pub max_depth: Option<usize>,
}

fn default_path() -> String {
    ".".to_string()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadSkeletonParams {
    /// File path relative to repo root.
    pub file: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadFullSymbolParams {
    /// File path relative to repo root.
    pub file: String,

    /// Symbol name to find (e.g., "skeletonize", "UserProfile"). Exact, case-sensitive match.
    pub symbol: String,

    /// Line number hint for disambiguation (1-indexed). Used when multiple symbols share the same name.
    #[serde(default)]
    pub start_line: Option<usize>,
}

#[derive(Clone)]
pub struct SkltnServer {
    root: PathBuf,
    tokenizer: Arc<CoreBPE>,
    tool_router: ToolRouter<Self>,
}

// Separate impl block for new() to avoid potential issues with the #[tool_router] macro
impl SkltnServer {
    pub fn new(root: PathBuf, tokenizer: CoreBPE) -> Self {
        Self {
            root,
            tokenizer: Arc::new(tokenizer),
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl SkltnServer {
    #[tool(description = "List the repository file structure as a tree. Shows supported source files (.rs, .py, .ts, .js) with byte sizes and detected languages. Use this to discover which files exist before reading them.")]
    async fn list_repo_structure(
        &self,
        Parameters(params): Parameters<ListRepoStructureParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let root = self.root.clone();
        let path = params.path;
        let max_depth = params.max_depth;

        let output = tokio::task::spawn_blocking(move || {
            // Validate path first
            match resolve_safe_path(&root, &path) {
                Ok(resolved) => {
                    if resolved.is_file() {
                        return McpError::PathIsFile(path).to_string();
                    }
                    if !resolved.is_dir() {
                        return McpError::DirectoryNotFound(path).to_string();
                    }
                    let tree = list_repo_structure::build_tree(&root, &path, max_depth);
                    if tree.trim().is_empty() {
                        McpError::NoSupportedFiles(path).to_string()
                    } else {
                        tree
                    }
                }
                Err(McpError::PathOutsideRoot) => McpError::PathOutsideRoot.to_string(),
                Err(_) => McpError::DirectoryNotFound(path).to_string(),
            }
        })
        .await
        .map_err(|e| ErrorData::internal_error(format!("Internal error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "Read a source file, returning either the full file (if ≤2000 tokens) or a skeletonized version (if >2000 tokens). Skeletons preserve signatures, types, and doc comments while replacing function bodies with placeholders. Use read_full_symbol to hydrate specific symbols.")]
    async fn read_skeleton(
        &self,
        Parameters(params): Parameters<ReadSkeletonParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let root = self.root.clone();
        let file = params.file;
        let tokenizer = Arc::clone(&self.tokenizer);

        let output = tokio::task::spawn_blocking(move || {
            read_skeleton::read_skeleton_or_full(&root, &file, &tokenizer)
        })
        .await
        .map_err(|e| ErrorData::internal_error(format!("Internal error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "Read the full, unmodified source code of a specific symbol (function, method, struct, class, enum, trait, interface, etc.). Returns the complete source text including doc comments and decorators. If multiple symbols share the same name, returns a disambiguation list — re-call with start_line to select one.")]
    async fn read_full_symbol(
        &self,
        Parameters(params): Parameters<ReadFullSymbolParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let root = self.root.clone();
        let file = params.file;
        let symbol = params.symbol;
        let start_line = params.start_line;
        let tokenizer = Arc::clone(&self.tokenizer);

        let output = tokio::task::spawn_blocking(move || {
            read_full_symbol::read_full_symbol(&root, &file, &symbol, start_line, &tokenizer)
        })
        .await
        .map_err(|e| ErrorData::internal_error(format!("Internal error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}

#[tool_handler]
impl ServerHandler for SkltnServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        .with_server_info(Implementation::from_build_env())
        .with_protocol_version(ProtocolVersion::V_2024_11_05)
        .with_instructions(
            "Skeleton (skltn) MCP server. Navigate codebases efficiently: \
             list_repo_structure → read_skeleton → read_full_symbol."
                .to_string(),
        )
    }
}
```

- [ ] **Step 2: Write the server bootstrap in main.rs**

Replace `crates/skltn-mcp/src/main.rs` with:
```rust
pub mod error;
pub mod budget;
pub mod resolve;
pub mod tools;

use std::path::PathBuf;
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Log to stderr — stdout is the MCP transport
    // RUST_LOG env var controls log level (e.g., RUST_LOG=debug)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    // Parse CLI arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: skltn-mcp <ROOT_PATH>");
        std::process::exit(1);
    }

    let root = PathBuf::from(&args[1]);
    if !root.is_dir() {
        eprintln!("Error: '{}' is not a valid directory", root.display());
        std::process::exit(1);
    }

    let root = root.canonicalize()?;

    tracing::info!("Starting skltn-mcp server with root: {}", root.display());

    // Initialize tokenizer once (shared across all tool calls)
    let tokenizer = tiktoken_rs::cl100k_base()
        .map_err(|e| format!("Failed to initialize tokenizer: {e}"))?;

    // Create server and serve over stdio
    let server = tools::SkltnServer::new(root, tokenizer);
    let service = server
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!("serving error: {:?}", e))?;

    service.waiting().await?;
    Ok(())
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p skltn-mcp`
Expected: Successful compilation.

- [ ] **Step 4: Verify all existing tests still pass**

Run: `cargo test -p skltn-mcp`
Expected: All tests pass (budget, path security, resolve, list_repo_structure, read_skeleton, read_full_symbol).

- [ ] **Step 5: Commit**

```bash
git add crates/skltn-mcp/src/tools/mod.rs crates/skltn-mcp/src/main.rs
git commit -m "feat(mcp): wire up SkltnServer with rmcp tool registration and stdio transport"
```

---

### Task 11: Add MCP Integration Tests

**Files:**
- Create: `crates/skltn-mcp/tests/mcp_integration_test.rs`

These tests validate the tool handlers work correctly through the `SkltnServer` struct (without a live stdio transport).

- [ ] **Step 1: Write the integration test**

`crates/skltn-mcp/tests/mcp_integration_test.rs`:
```rust
use std::fs;
use std::sync::Arc;
use std::path::PathBuf;
use skltn_mcp::tools::SkltnServer;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::*,
};

fn setup_server(root: &std::path::Path) -> SkltnServer {
    let tokenizer = tiktoken_rs::cl100k_base().unwrap();
    SkltnServer::new(root.canonicalize().unwrap(), tokenizer)
}

fn extract_text(result: CallToolResult) -> String {
    result
        .content
        .into_iter()
        .filter_map(|c| match c.raw {
            RawContent::Text(TextContent { text }) => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn create_test_repo(root: &std::path::Path) {
    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("main.rs"),
        r#"/// Entry point
fn main() {
    println!("hello");
}
"#,
    ).unwrap();
    fs::write(
        src.join("lib.rs"),
        r#"pub struct Config {
    pub host: String,
}

impl Config {
    pub fn new(host: String) -> Self {
        Config { host }
    }

    pub fn validate(&self) -> bool {
        !self.host.is_empty()
    }
}
"#,
    ).unwrap();
}

#[tokio::test]
async fn test_list_repo_structure_tool() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());
    let server = setup_server(dir.path());

    let params = skltn_mcp::tools::ListRepoStructureParams {
        path: ".".to_string(),
        max_depth: None,
    };

    let result = server.list_repo_structure(Parameters(params)).await.unwrap();
    let text = extract_text(result);
    assert!(text.contains("src/"));
    assert!(text.contains("main.rs"));
    assert!(text.contains("lib.rs"));
}

#[tokio::test]
async fn test_read_skeleton_tool() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());
    let server = setup_server(dir.path());

    let params = skltn_mcp::tools::ReadSkeletonParams {
        file: "src/main.rs".to_string(),
    };

    let result = server.read_skeleton(Parameters(params)).await.unwrap();
    let text = extract_text(result);
    assert!(text.contains("[file: src/main.rs"));
    assert!(text.contains("fn main()"));
}

#[tokio::test]
async fn test_read_full_symbol_tool() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());
    let server = setup_server(dir.path());

    let params = skltn_mcp::tools::ReadFullSymbolParams {
        file: "src/lib.rs".to_string(),
        symbol: "new".to_string(),
        start_line: None,
    };

    let result = server.read_full_symbol(Parameters(params)).await.unwrap();
    let text = extract_text(result);
    assert!(text.contains("[symbol: new"));
    assert!(text.contains("Config { host }"));
}

#[tokio::test]
async fn test_read_full_symbol_data_node() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());
    let server = setup_server(dir.path());

    let params = skltn_mcp::tools::ReadFullSymbolParams {
        file: "src/lib.rs".to_string(),
        symbol: "Config".to_string(),
        start_line: None,
    };

    let result = server.read_full_symbol(Parameters(params)).await.unwrap();
    let text = extract_text(result);
    assert!(text.contains("[symbol: Config"));
    assert!(text.contains("pub host: String"));
}

#[tokio::test]
async fn test_path_traversal_blocked_in_tool() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());
    let server = setup_server(dir.path());

    let params = skltn_mcp::tools::ReadSkeletonParams {
        file: "../../../etc/passwd".to_string(),
    };

    let result = server.read_skeleton(Parameters(params)).await.unwrap();
    let text = extract_text(result);
    assert!(
        text.contains("Path is outside the repository root")
        || text.contains("File not found")
    );
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test -p skltn-mcp --test mcp_integration_test`
Expected: All 5 tests pass.

Note: If the tool methods are private (not accessible from integration tests), the tests should instead call the underlying logic functions directly. The `#[tool]` macro methods may require the `SkltnServer` to be called in a specific way. If compilation errors occur because `list_repo_structure`, `read_skeleton`, `read_full_symbol` methods are not public, adjust by testing through the public logic functions (`list_repo_structure::build_tree`, `read_skeleton::read_skeleton_or_full`, `read_full_symbol::read_full_symbol`) which are already tested in earlier tasks. In that case, simplify this test to just verify the server compiles and can be constructed:

```rust
#[tokio::test]
async fn test_server_construction() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let tokenizer = tiktoken_rs::cl100k_base().unwrap();
    let _server = skltn_mcp::tools::SkltnServer::new(root, tokenizer);
    // Server constructs successfully with tool router initialized
}
```

- [ ] **Step 3: Commit**

```bash
git add crates/skltn-mcp/tests/mcp_integration_test.rs
git commit -m "test(mcp): add MCP integration tests for all three tools"
```

---

## Chunk 6: Final Validation and Cleanup

### Task 12: Run Full Test Suite and Verify Build

**Files:**
- No new files — validation only

- [ ] **Step 1: Run all tests**

Run: `cargo test --workspace`
Expected: All tests across all crates pass.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings or errors.

- [ ] **Step 3: Fix any clippy warnings**

If clippy reports issues, fix them. Common things to check:
- Unused imports
- Unnecessary clones
- Missing `pub` on items that should be public
- Dead code warnings

- [ ] **Step 4: Verify the binary runs**

Run: `cargo run -p skltn-mcp -- .`
Expected: The server starts (blocking on stdin for MCP messages). Press Ctrl+C to stop. Should see no errors on stderr.

Note: It will wait for MCP JSON-RPC input on stdin. Simply verify it doesn't crash on startup.

- [ ] **Step 5: Commit any fixes**

```bash
git add -A
git commit -m "fix(mcp): address clippy warnings and ensure clean build"
```

---

### Task 13: Final Full Validation

**Files:**
- No new files — final check

- [ ] **Step 1: Run entire workspace test suite**

Run: `cargo test --workspace`
Expected: All tests pass across skltn-core, skltn-cli, and skltn-mcp.

- [ ] **Step 2: Run clippy one final time**

Run: `cargo clippy --workspace -- -D warnings`
Expected: Clean.

- [ ] **Step 3: Verify binary starts correctly**

Run: `cargo run -p skltn-mcp -- .`
Expected: Server starts without errors. Ctrl+C to exit.

- [ ] **Step 4: Commit any final fixes**

If any fixes were needed:
```bash
git add -A
git commit -m "fix(mcp): final cleanup for Phase 2"
```

---

## Summary

| Chunk | Tasks | What It Delivers |
|---|---|---|
| 1 | Tasks 1-3 | Crate scaffolding, McpError types, Budget Guard with tests |
| 2 | Tasks 4-6 | Path security, symbol resolution with scope stack and doc comment look-back, TS data node tests |
| 3 | Tasks 7-8 | list_repo_structure and read_skeleton tool logic with shared utilities and tests |
| 4 | Task 9 | read_full_symbol tool logic with tests |
| 5 | Tasks 10-11 | rmcp server wiring, tool registration, stdio bootstrap, integration tests |
| 6 | Tasks 12-13 | Clippy, full validation |

**Total:** 13 tasks, ~42 steps. Each task produces a working, testable increment with a commit.
