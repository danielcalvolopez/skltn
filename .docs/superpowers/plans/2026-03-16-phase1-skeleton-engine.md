# Phase 1: Skeleton Engine Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust library and CLI that uses tree-sitter AST parsing to skeletonize source files (Rust, Python, TypeScript, JavaScript) — stripping function bodies while preserving architectural signatures with syntactic validity.

**Architecture:** Cargo workspace with two crates: `skltn-core` (library with trait-based language backends and a stateless engine) and `skltn-cli` (thin CLI wrapper using clap + ignore). The engine walks tree-sitter ASTs, classifying nodes as structural (prune body) or pass-through (emit verbatim), applying byte-range replacements in reverse order.

**Tech Stack:** Rust (latest stable), tree-sitter (with per-language grammar crates), clap, ignore, is-terminal, insta (snapshot testing)

**Spec:** `docs/superpowers/specs/2026-03-16-phase1-skeleton-engine-design.md`

---

## Chunk 1: Project Scaffolding, Trait, Error Types, and Options

### Task 1: Initialize Cargo Workspace

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/skltn-core/Cargo.toml`
- Create: `crates/skltn-core/src/lib.rs`
- Create: `crates/skltn-cli/Cargo.toml`
- Create: `crates/skltn-cli/src/main.rs`

- [ ] **Step 1: Create workspace root Cargo.toml**

```toml
[workspace]
resolver = "2"
members = ["crates/skltn-core", "crates/skltn-cli"]
```

- [ ] **Step 2: Create skltn-core crate**

`crates/skltn-core/Cargo.toml`:
```toml
[package]
name = "skltn-core"
version = "0.1.0"
edition = "2021"

[dependencies]
tree-sitter = "0.24"
tree-sitter-rust = "0.23"
tree-sitter-python = "0.23"
tree-sitter-typescript = "0.23"
tree-sitter-javascript = "0.23"
thiserror = "2"

[dev-dependencies]
insta = "1"
```

`crates/skltn-core/src/lib.rs`:
```rust
pub mod backend;
pub mod engine;
pub mod error;
pub mod options;
```

- [ ] **Step 3: Create skltn-cli crate**

`crates/skltn-cli/Cargo.toml`:
```toml
[package]
name = "skltn-cli"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "skltn"
path = "src/main.rs"

[dependencies]
skltn-core = { path = "../skltn-core" }
clap = { version = "4", features = ["derive"] }
ignore = "0.4"
is-terminal = "0.4"
```

`crates/skltn-cli/src/main.rs`:
```rust
fn main() {
    println!("skltn CLI - not yet implemented");
}
```

- [ ] **Step 4: Verify workspace compiles**

Run: `cargo build`
Expected: Successful compilation with no errors.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/
git commit -m "chore: initialize cargo workspace with skltn-core and skltn-cli crates"
```

---

### Task 2: Define Error Types

**Files:**
- Create: `crates/skltn-core/src/error.rs`

- [ ] **Step 1: Write error types**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkltnError {
    #[error("unsupported language for extension: {0}")]
    UnsupportedLanguage(String),

    #[error("failed to parse source: {0}")]
    ParseError(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p skltn-core`
Expected: Successful compilation.

- [ ] **Step 3: Commit**

```bash
git add crates/skltn-core/src/error.rs
git commit -m "feat(core): add SkltnError type with thiserror"
```

---

### Task 3: Define SkeletonOptions

**Files:**
- Create: `crates/skltn-core/src/options.rs`

- [ ] **Step 1: Write options struct**

```rust
/// Configuration for the skeletonization process.
#[derive(Debug, Clone)]
pub struct SkeletonOptions {
    /// Maximum nesting depth of leaf structural nodes to skeletonize.
    /// None means unlimited depth.
    pub max_depth: Option<usize>,
}

impl Default for SkeletonOptions {
    fn default() -> Self {
        Self { max_depth: None }
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p skltn-core`
Expected: Successful compilation.

- [ ] **Step 3: Commit**

```bash
git add crates/skltn-core/src/options.rs
git commit -m "feat(core): add SkeletonOptions with configurable max_depth"
```

---

### Task 4: Define the LanguageBackend Trait

**Files:**
- Create: `crates/skltn-core/src/backend/mod.rs`

- [ ] **Step 1: Write the trait definition**

```rust
use tree_sitter::{Language, Node};

/// Trait that each supported language must implement.
/// The engine delegates all language-specific AST decisions to this trait.
///
/// Two categories of structural nodes exist:
/// - "Leaf" structural nodes (functions, methods) have a body to prune.
///   `body_node()` returns `Some(body)` for these.
/// - "Container" structural nodes (impl blocks, classes, modules) have no body
///   to prune, but the engine recurses into their children.
///   `body_node()` returns `None` for these.
pub trait LanguageBackend {
    /// Returns the tree-sitter Language grammar.
    fn language(&self) -> Language;

    /// File extensions this backend handles (e.g., &["rs"]).
    fn extensions(&self) -> &[&str];

    /// Is this AST node a structural node (leaf or container)?
    fn is_structural_node(&self, node: &Node) -> bool;

    /// Is this a doc comment node that should be preserved?
    /// Source bytes are needed because tree-sitter node kinds alone can't
    /// distinguish `///` doc comments from `//` regular comments.
    fn is_doc_comment(&self, node: &Node, source: &[u8]) -> bool;

    /// Given a structural node, return the child node representing the body.
    /// Returns Some(body) for leaf structural nodes (functions, methods).
    /// Returns None for container nodes (impl blocks, classes) and abstract methods.
    fn body_node<'a>(&self, node: &Node<'a>) -> Option<Node<'a>>;

    /// Returns the idiomatic placeholder for this language.
    /// e.g., "todo!()" for Rust, "pass" for Python.
    fn placeholder(&self) -> &str;

    /// Returns the formatted line-count tag comment.
    /// e.g., "// [skltn: 47 lines hidden]" for Rust.
    fn hidden_line_tag(&self, count: usize) -> String;

    /// Format the replacement text for a pruned body.
    /// This is language-specific because brace-delimited languages (Rust, JS, TS)
    /// need `{ placeholder }` while indentation-based languages (Python)
    /// need just the indented placeholder.
    /// `indent` is the whitespace string matching the body's indentation level.
    /// `body` and `source` are provided so backends can extract leading docstrings (Python).
    fn format_replacement(&self, indent: &str, line_count: usize, body: &Node, source: &[u8]) -> String;
}
```

- [ ] **Step 2: Update lib.rs to expose backend module**

Ensure `crates/skltn-core/src/lib.rs` has `pub mod backend;` (already added in Task 1).

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p skltn-core`
Expected: Successful compilation.

- [ ] **Step 4: Commit**

```bash
git add crates/skltn-core/src/backend/
git commit -m "feat(core): define LanguageBackend trait with container/leaf node distinction"
```

---

## Chunk 2: Rust Backend + Engine Core

### Task 5: Create Rust Test Fixtures

**Files:**
- Create: `fixtures/rust/simple_function.rs`
- Create: `fixtures/rust/struct_with_methods.rs`
- Create: `fixtures/rust/enums_and_constants.rs`
- Create: `fixtures/rust/doc_comments.rs`

- [ ] **Step 1: Create simple_function.rs fixture**

```rust
use std::collections::HashMap;

pub fn add(a: i32, b: i32) -> i32 {
    let result = a + b;
    println!("Adding {} + {} = {}", a, b, result);
    result
}

fn helper(x: &str) -> String {
    let mut s = String::from(x);
    s.push_str("_processed");
    s.to_uppercase()
}
```

- [ ] **Step 2: Create struct_with_methods.rs fixture**

```rust
use std::fmt;

/// A token counter that tracks usage statistics.
pub struct TokenCounter {
    pub raw_count: u64,
    pub skeleton_count: u64,
    compression_ratio: f64,
}

impl TokenCounter {
    /// Creates a new TokenCounter with zero counts.
    pub fn new() -> Self {
        Self {
            raw_count: 0,
            skeleton_count: 0,
            compression_ratio: 0.0,
        }
    }

    /// Records a raw token count and updates the ratio.
    pub fn record(&mut self, raw: u64, skeleton: u64) {
        self.raw_count += raw;
        self.skeleton_count += skeleton;
        if self.raw_count > 0 {
            self.compression_ratio =
                1.0 - (self.skeleton_count as f64 / self.raw_count as f64);
        }
    }

    /// Returns the compression ratio as a percentage.
    pub fn ratio(&self) -> f64 {
        self.compression_ratio * 100.0
    }
}

impl fmt::Display for TokenCounter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Tokens: {} raw, {} skeleton ({:.1}% compression)",
            self.raw_count,
            self.skeleton_count,
            self.ratio()
        )
    }
}
```

- [ ] **Step 3: Create enums_and_constants.rs fixture**

```rust
pub const MAX_FILE_SIZE: usize = 1_048_576;
pub const DEFAULT_DEPTH: usize = 100;

/// Errors that can occur during parsing.
#[derive(Debug)]
pub enum ParseError {
    UnsupportedLanguage(String),
    SyntaxError { line: usize, col: usize },
    IoError(std::io::Error),
}

/// Supported programming languages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Language {
    Rust,
    Python,
    TypeScript,
    JavaScript,
}

impl Language {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Self::Rust),
            "py" => Some(Self::Python),
            "ts" => Some(Self::TypeScript),
            "js" => Some(Self::JavaScript),
            _ => None,
        }
    }
}
```

- [ ] **Step 4: Create doc_comments.rs fixture**

```rust
/// This module handles authentication logic.
///
/// It provides token-based auth with refresh capabilities.

use std::time::Duration;

/// Validates a JWT token and returns the decoded claims.
///
/// # Arguments
/// * `token` - The JWT string to validate
/// * `secret` - The signing secret
///
/// # Returns
/// The decoded claims if valid, or an error.
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, AuthError> {
    // First decode the header
    let header = decode_header(token)?;
    // Then verify the signature
    let claims = verify_signature(token, secret, &header)?;
    // Check expiration
    if claims.exp < current_timestamp() {
        return Err(AuthError::Expired);
    }
    Ok(claims)
}

/// Represents decoded JWT claims.
pub struct Claims {
    pub sub: String,
    pub exp: u64,
    pub iat: u64,
}

/// Authentication errors.
#[derive(Debug)]
pub enum AuthError {
    /// The token has expired.
    Expired,
    /// The signature is invalid.
    InvalidSignature,
    /// The token format is malformed.
    Malformed(String),
}
```

- [ ] **Step 5: Commit fixtures**

```bash
git add fixtures/rust/
git commit -m "test(fixtures): add Rust test fixtures for basic skeletonization"
```

---

### Task 6: Implement RustBackend

**Files:**
- Create: `crates/skltn-core/src/backend/rust.rs`
- Modify: `crates/skltn-core/src/backend/mod.rs` (add `pub mod rust;`)

- [ ] **Step 1: Write the RustBackend**

```rust
use tree_sitter::{Language, Node};

use super::LanguageBackend;

pub struct RustBackend;

impl LanguageBackend for RustBackend {
    fn language(&self) -> Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn extensions(&self) -> &[&str] {
        &["rs"]
    }

    fn is_structural_node(&self, node: &Node) -> bool {
        matches!(
            node.kind(),
            "function_item"
                | "impl_item"
                | "trait_item"
                | "mod_item"
                | "closure_expression"
        )
    }

    fn is_doc_comment(&self, node: &Node, source: &[u8]) -> bool {
        matches!(node.kind(), "line_comment" | "block_comment")
            && node
                .utf8_text(source)
                .map(|t| t.starts_with("///") || t.starts_with("//!") || t.starts_with("/**"))
                .unwrap_or(false)
    }

    fn body_node<'a>(&self, node: &Node<'a>) -> Option<Node<'a>> {
        match node.kind() {
            // Leaf structural nodes — have a body to prune
            "function_item" => node.child_by_field_name("body"),
            "closure_expression" => {
                // Only prune block-bodied closures
                node.child_by_field_name("body")
                    .filter(|body| body.kind() == "block")
            }
            // Container structural nodes — recurse, don't prune
            "impl_item" | "trait_item" | "mod_item" => None,
            _ => None,
        }
    }

    fn placeholder(&self) -> &str {
        "todo!()"
    }

    fn hidden_line_tag(&self, count: usize) -> String {
        format!("// [skltn: {} lines hidden]", count)
    }

    fn format_replacement(&self, indent: &str, line_count: usize, _body: &Node, _source: &[u8]) -> String {
        // For Rust, replace the entire block node: { placeholder // tag }
        let inner_indent = format!("{}    ", indent);
        format!(
            "{{\n{}{} {}\n{}}}",
            inner_indent,
            self.placeholder(),
            self.hidden_line_tag(line_count),
            indent,
        )
    }
}
```

- [ ] **Step 2: Update backend/mod.rs to include rust module**

Add `pub mod rust;` to `crates/skltn-core/src/backend/mod.rs`.

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p skltn-core`
Expected: Successful compilation.

- [ ] **Step 4: Commit**

```bash
git add crates/skltn-core/src/backend/rust.rs crates/skltn-core/src/backend/mod.rs
git commit -m "feat(core): implement RustBackend for tree-sitter AST classification"
```

---

### Task 7: Implement the Skeleton Engine

**Files:**
- Create: `crates/skltn-core/src/engine.rs`

- [ ] **Step 1: Write the engine**

```rust
use tree_sitter::{Node, Parser};

use crate::backend::LanguageBackend;
use crate::error::SkltnError;
use crate::options::SkeletonOptions;

/// A replacement to apply to the source text.
struct Replacement {
    start: usize,
    end: usize,
    text: String,
}

pub struct SkeletonEngine;

impl SkeletonEngine {
    /// Skeletonize source code using the given language backend.
    pub fn skeletonize(
        source: &str,
        backend: &dyn LanguageBackend,
        options: &SkeletonOptions,
    ) -> Result<String, SkltnError> {
        let mut parser = Parser::new();
        parser
            .set_language(&backend.language())
            .map_err(|e| SkltnError::ParseError(e.to_string()))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| SkltnError::ParseError("tree-sitter returned no tree".into()))?;

        let mut replacements = Vec::new();
        Self::walk_node(
            &tree.root_node(),
            source.as_bytes(),
            backend,
            options,
            0, // current depth
            &mut replacements,
        );

        // Apply replacements in reverse order to preserve byte offsets
        replacements.sort_by(|a, b| b.start.cmp(&a.start));

        let mut result = source.to_string();
        for rep in &replacements {
            result.replace_range(rep.start..rep.end, &rep.text);
        }

        Ok(result)
    }

    fn walk_node(
        node: &Node,
        source: &[u8],
        backend: &dyn LanguageBackend,
        options: &SkeletonOptions,
        depth: usize,
        replacements: &mut Vec<Replacement>,
    ) {
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if child.kind() == "ERROR" {
                // Emit verbatim — no replacement needed, just skip recursion
                continue;
            }

            if backend.is_structural_node(&child) {
                match backend.body_node(&child) {
                    Some(body) => {
                        // Leaf structural node — prune body
                        if let Some(max) = options.max_depth {
                            if depth >= max {
                                // At max depth: emit verbatim, don't recurse
                                continue;
                            }
                        }

                        let line_count =
                            body.end_position().row - body.start_position().row + 1;

                        // Calculate indentation: use the first line of the body
                        // to determine the indentation level
                        let body_start_line_start = source[..body.start_byte()]
                            .iter()
                            .rposition(|&b| b == b'\n')
                            .map(|p| p + 1)
                            .unwrap_or(0);
                        let indent_str: String = source[body_start_line_start..body.start_byte()]
                            .iter()
                            .take_while(|&&b| b == b' ' || b == b'\t')
                            .map(|&b| b as char)
                            .collect();

                        // Delegate formatting to the backend (handles brace vs indentation languages)
                        let replacement_text = backend.format_replacement(&indent_str, line_count, &body, source);

                        replacements.push(Replacement {
                            start: body.start_byte(),
                            end: body.end_byte(),
                            text: replacement_text,
                        });

                        // Do NOT recurse into this node's children — the body is being
                        // replaced, and any nested functions inside it are intentionally hidden.
                        // Non-body children (parameters, return types) are part of the
                        // signature and are preserved by the byte-range replacement
                        // (they are outside the body's byte range).
                    }
                    None => {
                        // Container structural node (impl, class, module) or abstract method.
                        // Recurse into children without incrementing depth.
                        Self::walk_node(&child, source, backend, options, depth, replacements);
                    }
                }
            } else {
                // Non-structural node — emit verbatim, skip subtree.
                // No replacement needed, no recursion needed.
            }
        }
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p skltn-core`
Expected: Successful compilation.

- [ ] **Step 3: Commit**

```bash
git add crates/skltn-core/src/engine.rs
git commit -m "feat(core): implement SkeletonEngine with byte-range replacement strategy"
```

---

### Task 8: Create Shared Test Utilities

**Files:**
- Create: `crates/skltn-core/tests/common/mod.rs`

- [ ] **Step 1: Write shared test helpers**

```rust
use skltn_core::backend::LanguageBackend;
use skltn_core::options::SkeletonOptions;

pub fn default_opts() -> SkeletonOptions {
    SkeletonOptions::default()
}

/// Recursively check if any node in the tree is an ERROR or MISSING node.
pub fn has_error_nodes(node: &tree_sitter::Node) -> bool {
    if node.is_error() || node.is_missing() {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_error_nodes(&child) {
            return true;
        }
    }
    false
}

/// Assert that the skeleton output is syntactically valid by re-parsing it.
pub fn assert_valid_syntax(skeleton: &str, backend: &dyn LanguageBackend) {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&backend.language()).unwrap();
    let tree = parser.parse(skeleton, None).unwrap();
    assert!(
        !has_error_nodes(&tree.root_node()),
        "Skeleton output has syntax errors:\n{}",
        skeleton
    );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo test -p skltn-core --no-run`
Expected: Successful compilation.

- [ ] **Step 3: Commit**

```bash
git add crates/skltn-core/tests/common/
git commit -m "test(core): add shared test utilities for snapshot and round-trip testing"
```

---

### Task 9: Write Snapshot Tests for Rust Backend

**Files:**
- Create: `crates/skltn-core/tests/rust_backend.rs`

- [ ] **Step 1: Write snapshot tests**

```rust
mod common;

use common::{assert_valid_syntax, default_opts};
use skltn_core::backend::rust::RustBackend;
use skltn_core::engine::SkeletonEngine;

#[test]
fn test_rust_simple_function() {
    let source = include_str!("../../../fixtures/rust/simple_function.rs");
    let backend = RustBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_rust_struct_with_methods() {
    let source = include_str!("../../../fixtures/rust/struct_with_methods.rs");
    let backend = RustBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_rust_enums_and_constants() {
    let source = include_str!("../../../fixtures/rust/enums_and_constants.rs");
    let backend = RustBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_rust_doc_comments() {
    let source = include_str!("../../../fixtures/rust/doc_comments.rs");
    let backend = RustBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_rust_simple_function_valid_syntax() {
    let source = include_str!("../../../fixtures/rust/simple_function.rs");
    let backend = RustBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    assert_valid_syntax(&result, &backend);
}

#[test]
fn test_rust_struct_with_methods_valid_syntax() {
    let source = include_str!("../../../fixtures/rust/struct_with_methods.rs");
    let backend = RustBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    assert_valid_syntax(&result, &backend);
}

#[test]
fn test_rust_doc_comments_valid_syntax() {
    let source = include_str!("../../../fixtures/rust/doc_comments.rs");
    let backend = RustBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    assert_valid_syntax(&result, &backend);
}
```

- [ ] **Step 2: Run tests to generate initial snapshots**

Run: `cargo test -p skltn-core`
Expected: Tests run. Some may fail on first run because snapshots don't exist yet.

- [ ] **Step 3: Review and accept snapshots**

Run: `cargo insta review`
Expected: Review each snapshot output. Accept if the skeleton correctly shows signatures with `todo!() // [skltn: N lines hidden]` and preserves structs/enums/constants verbatim.

- [ ] **Step 4: Run tests again to confirm all pass**

Run: `cargo test -p skltn-core`
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/skltn-core/tests/ crates/skltn-core/src/
git commit -m "test(core): add Rust backend snapshot tests with round-trip validation"
```

---

## Chunk 3: Python Backend

### Task 10: Create Python Test Fixtures

**Files:**
- Create: `fixtures/python/simple_function.py`
- Create: `fixtures/python/class_with_methods.py`
- Create: `fixtures/python/docstrings.py`
- Create: `fixtures/python/deeply_nested.py`

- [ ] **Step 1: Create simple_function.py fixture**

```python
from pathlib import Path
import json


def read_config(path: str) -> dict:
    """Read and parse a JSON configuration file."""
    with open(path) as f:
        data = json.load(f)
    validated = validate_config(data)
    return validated


def validate_config(data: dict) -> dict:
    required_keys = ["name", "version", "entries"]
    for key in required_keys:
        if key not in data:
            raise ValueError(f"Missing required key: {key}")
    if not isinstance(data["entries"], list):
        raise TypeError("entries must be a list")
    return data
```

- [ ] **Step 2: Create class_with_methods.py fixture**

```python
from typing import Optional
import logging

logger = logging.getLogger(__name__)

TIMEOUT = 30
MAX_RETRIES = 3


class UserService:
    """Manages user operations and authentication."""

    def __init__(self, db_url: str, timeout: int = TIMEOUT):
        """Initialize the service with a database connection."""
        self.db_url = db_url
        self.timeout = timeout
        self._connection = None
        self._cache: dict = {}
        logger.info(f"UserService initialized with {db_url}")

    def authenticate(self, token: str) -> bool:
        """Validate a user token against the database."""
        decoded = self._decode_token(token)
        if not decoded:
            return False
        user = self._fetch_user(decoded["sub"])
        if user and user.is_active:
            self._cache[token] = user
            return True
        return False

    def get_user(self, user_id: int) -> Optional[dict]:
        """Fetch a user by ID."""
        if user_id in self._cache:
            return self._cache[user_id]
        result = self._query_db(
            "SELECT * FROM users WHERE id = %s",
            (user_id,)
        )
        return result

    def _decode_token(self, token: str) -> Optional[dict]:
        try:
            import jwt
            return jwt.decode(token, self._secret, algorithms=["HS256"])
        except Exception as e:
            logger.error(f"Token decode failed: {e}")
            return None
```

- [ ] **Step 3: Create docstrings.py fixture**

```python
"""
Module for handling payment processing.

This module provides the core payment gateway integration
with support for multiple providers.
"""

from decimal import Decimal
from typing import Optional


class PaymentProcessor:
    """Processes payments through various gateways.

    Supports Stripe, PayPal, and direct bank transfers.
    Implements retry logic and idempotency.
    """

    def process(self, amount: Decimal, currency: str = "USD") -> str:
        """Process a payment and return the transaction ID.

        Args:
            amount: The payment amount.
            currency: ISO 4217 currency code.

        Returns:
            A unique transaction identifier.

        Raises:
            PaymentError: If the payment fails.
        """
        validated_amount = self._validate_amount(amount)
        gateway = self._select_gateway(currency)
        result = gateway.charge(validated_amount, currency)
        self._record_transaction(result)
        return result.transaction_id
```

- [ ] **Step 4: Create deeply_nested.py fixture**

```python
class OuterService:
    """Service with deeply nested structures."""

    class Config:
        """Nested configuration."""
        timeout: int = 30
        retries: int = 3

    def process(self, data: dict) -> dict:
        """Process data with error handling."""
        try:
            if data.get("type") == "complex":
                for item in data["items"]:
                    if item.get("nested"):
                        for sub in item["nested"]:
                            result = self._transform(sub)
                            if not result:
                                raise ValueError(f"Transform failed for {sub}")
                            self._store(result)
            else:
                return self._simple_process(data)
        except KeyError as e:
            logger.error(f"Missing key: {e}")
            raise
        except ValueError:
            return {"status": "error", "data": data}
        return {"status": "ok"}

    def _transform(self, item: dict) -> Optional[dict]:
        mapped = {}
        for key, value in item.items():
            if isinstance(value, str):
                mapped[key] = value.strip().lower()
            elif isinstance(value, (int, float)):
                mapped[key] = value * 1.1
            else:
                mapped[key] = str(value)
        return mapped if mapped else None
```

- [ ] **Step 5: Commit fixtures**

```bash
git add fixtures/python/
git commit -m "test(fixtures): add Python test fixtures"
```

---

### Task 11: Implement PythonBackend

**Files:**
- Create: `crates/skltn-core/src/backend/python.rs`
- Modify: `crates/skltn-core/src/backend/mod.rs` (add `pub mod python;`)

- [ ] **Step 1: Write the PythonBackend**

```rust
use tree_sitter::{Language, Node};

use super::LanguageBackend;

pub struct PythonBackend;

impl PythonBackend {
    /// Extract the leading docstring from a Python function body block, if present.
    /// Returns the docstring text (including quotes) and its byte range.
    pub fn extract_docstring<'a>(body: &Node<'a>, source: &[u8]) -> Option<String> {
        // In Python's tree-sitter AST, the body is a `block` node.
        // A docstring is the first child if it's an `expression_statement`
        // containing a `string` node.
        let first_child = body.child(0)?;
        if first_child.kind() != "expression_statement" {
            return None;
        }
        let string_node = first_child.child(0)?;
        if string_node.kind() != "string" {
            return None;
        }
        let text = string_node.utf8_text(source).ok()?;
        // Only triple-quoted strings are docstrings
        if text.starts_with("\"\"\"") || text.starts_with("'''") {
            Some(first_child.utf8_text(source).ok()?.to_string())
        } else {
            None
        }
    }
}

impl LanguageBackend for PythonBackend {
    fn language(&self) -> Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn extensions(&self) -> &[&str] {
        &["py"]
    }

    fn is_structural_node(&self, node: &Node) -> bool {
        matches!(
            node.kind(),
            "function_definition" | "class_definition"
        )
    }

    fn is_doc_comment(&self, node: &Node, _source: &[u8]) -> bool {
        // Python standalone comments (# ...) are always preserved
        // as non-structural nodes (they pass through verbatim).
        // Docstrings inside function bodies are handled specially
        // by extract_docstring() in format_replacement().
        node.kind() == "comment"
    }

    fn body_node<'a>(&self, node: &Node<'a>) -> Option<Node<'a>> {
        match node.kind() {
            // Leaf structural node — has body to prune
            "function_definition" => node.child_by_field_name("body"),
            // Container structural node — recurse into children
            "class_definition" => None,
            _ => None,
        }
    }

    fn placeholder(&self) -> &str {
        "pass"
    }

    fn hidden_line_tag(&self, count: usize) -> String {
        format!("# [skltn: {} lines hidden]", count)
    }

    fn format_replacement(&self, indent: &str, line_count: usize, body: &Node, source: &[u8]) -> String {
        // Python has no braces — the body is an indented block.
        // Extract and preserve leading docstrings before replacing the body.
        // Note: verify during snapshot review that no double blank line appears
        // between the function signature and the placeholder. If tree-sitter's
        // block node includes the newline after the colon, remove the leading \n.
        let docstring = PythonBackend::extract_docstring(body, source);
        match docstring {
            Some(doc) => format!(
                "\n{}{}\n{}{} {}",
                indent, doc,
                indent, self.placeholder(), self.hidden_line_tag(line_count),
            ),
            None => format!(
                "\n{}{} {}",
                indent, self.placeholder(), self.hidden_line_tag(line_count),
            ),
        }
    }
}
```

- [ ] **Step 2: Update backend/mod.rs**

Add `pub mod python;` to `crates/skltn-core/src/backend/mod.rs`.

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p skltn-core`
Expected: Successful compilation.

- [ ] **Step 4: Commit**

```bash
git add crates/skltn-core/src/backend/python.rs crates/skltn-core/src/backend/mod.rs
git commit -m "feat(core): implement PythonBackend"
```

---

### Task 12: Write Snapshot Tests for Python Backend

**Files:**
- Create: `crates/skltn-core/tests/python_backend.rs`

- [ ] **Step 1: Write snapshot tests**

```rust
mod common;

use common::{assert_valid_syntax, default_opts};
use skltn_core::backend::python::PythonBackend;
use skltn_core::engine::SkeletonEngine;

#[test]
fn test_python_simple_function() {
    let source = include_str!("../../../fixtures/python/simple_function.py");
    let backend = PythonBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_python_class_with_methods() {
    let source = include_str!("../../../fixtures/python/class_with_methods.py");
    let backend = PythonBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_python_docstrings() {
    let source = include_str!("../../../fixtures/python/docstrings.py");
    let backend = PythonBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_python_deeply_nested() {
    let source = include_str!("../../../fixtures/python/deeply_nested.py");
    let backend = PythonBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_python_simple_function_valid_syntax() {
    let source = include_str!("../../../fixtures/python/simple_function.py");
    let backend = PythonBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    assert_valid_syntax(&result, &backend);
}

#[test]
fn test_python_class_with_methods_valid_syntax() {
    let source = include_str!("../../../fixtures/python/class_with_methods.py");
    let backend = PythonBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    assert_valid_syntax(&result, &backend);
}

#[test]
fn test_python_deeply_nested_valid_syntax() {
    let source = include_str!("../../../fixtures/python/deeply_nested.py");
    let backend = PythonBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    assert_valid_syntax(&result, &backend);
}
```

- [ ] **Step 2: Run tests, review and accept snapshots**

Run: `cargo test -p skltn-core`
Then: `cargo insta review`
Expected: Python skeletons show function signatures with `pass # [skltn: N lines hidden]` and class bodies with methods individually pruned.

- [ ] **Step 3: Run tests again to confirm all pass**

Run: `cargo test -p skltn-core`
Expected: All tests PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/skltn-core/tests/python_backend.rs
git commit -m "test(core): add Python backend snapshot tests with round-trip validation"
```

---

## Chunk 4: JavaScript and TypeScript Backends

### Task 13: Create JS/TS Test Fixtures

**Files:**
- Create: `fixtures/javascript/simple_function.js`
- Create: `fixtures/javascript/class_with_methods.js`
- Create: `fixtures/javascript/arrow_functions.js`
- Create: `fixtures/typescript/simple_function.ts`
- Create: `fixtures/typescript/interface_and_types.ts`
- Create: `fixtures/typescript/class_with_abstract.ts`
- Create: `fixtures/typescript/arrow_functions.ts`

- [ ] **Step 1: Create fixtures/javascript/simple_function.js**

```javascript
import { readFileSync } from 'fs';
import path from 'path';

/**
 * Reads and parses a JSON configuration file.
 * @param {string} filePath - Path to the config file.
 * @returns {Object} The parsed configuration.
 */
export function readConfig(filePath) {
    const fullPath = path.resolve(filePath);
    const raw = readFileSync(fullPath, 'utf-8');
    const parsed = JSON.parse(raw);
    if (!parsed.name || !parsed.version) {
        throw new Error('Invalid config: missing name or version');
    }
    return parsed;
}

function validateEntry(entry) {
    if (typeof entry.id !== 'number') {
        throw new TypeError('Entry id must be a number');
    }
    if (!entry.label || entry.label.trim() === '') {
        throw new Error('Entry label is required');
    }
    return true;
}
```

- [ ] **Step 2: Create fixtures/javascript/class_with_methods.js**

```javascript
import EventEmitter from 'events';

const DEFAULT_TIMEOUT = 5000;

/**
 * Manages WebSocket connections with auto-reconnect.
 */
export class ConnectionManager extends EventEmitter {
    /**
     * @param {string} url - The WebSocket URL.
     * @param {Object} options - Connection options.
     */
    constructor(url, options = {}) {
        super();
        this.url = url;
        this.timeout = options.timeout || DEFAULT_TIMEOUT;
        this._socket = null;
        this._retries = 0;
    }

    /**
     * Establish a connection to the server.
     * @returns {Promise<void>}
     */
    async connect() {
        this._socket = new WebSocket(this.url);
        this._socket.onopen = () => {
            this._retries = 0;
            this.emit('connected');
        };
        this._socket.onclose = () => {
            if (this._retries < 3) {
                this._retries++;
                setTimeout(() => this.connect(), this.timeout);
            }
        };
        this._socket.onerror = (err) => {
            this.emit('error', err);
        };
    }

    /**
     * Send a message through the connection.
     * @param {Object} data - The data to send.
     */
    send(data) {
        if (!this._socket) {
            throw new Error('Not connected');
        }
        const payload = JSON.stringify(data);
        this._socket.send(payload);
        this.emit('sent', data);
    }
}
```

- [ ] **Step 3: Create fixtures/javascript/arrow_functions.js**

```javascript
// Expression-bodied arrow functions (should NOT be pruned)
export const double = (x) => x * 2;
export const greet = (name) => `Hello, ${name}!`;
const identity = x => x;

// Block-bodied arrow functions (should be pruned)
export const processItems = (items) => {
    const results = [];
    for (const item of items) {
        if (item.active) {
            const transformed = {
                ...item,
                processedAt: Date.now(),
                label: item.label.toUpperCase(),
            };
            results.push(transformed);
        }
    }
    return results;
};

const fetchData = async (url) => {
    const response = await fetch(url);
    if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
    }
    const data = await response.json();
    return data;
};
```

- [ ] **Step 4: Create fixtures/typescript/simple_function.ts**

```typescript
import { readFileSync } from 'fs';
import path from 'path';

interface Config {
    name: string;
    version: string;
    entries: ConfigEntry[];
}

interface ConfigEntry {
    id: number;
    label: string;
    enabled: boolean;
}

type ConfigValidator = (config: Config) => boolean;

export function readConfig(filePath: string): Config {
    const fullPath = path.resolve(filePath);
    const raw = readFileSync(fullPath, 'utf-8');
    const parsed: Config = JSON.parse(raw);
    if (!validateConfig(parsed)) {
        throw new Error('Invalid configuration');
    }
    return parsed;
}

function validateConfig(config: Config): boolean {
    if (!config.name || !config.version) {
        return false;
    }
    if (!Array.isArray(config.entries)) {
        return false;
    }
    return config.entries.every(e => typeof e.id === 'number');
}
```

- [ ] **Step 5: Create fixtures/typescript/interface_and_types.ts**

```typescript
export interface User {
    id: number;
    name: string;
    email: string;
    role: UserRole;
    metadata?: Record<string, unknown>;
}

export type UserRole = 'admin' | 'editor' | 'viewer';

export type Result<T, E = Error> =
    | { ok: true; value: T }
    | { ok: false; error: E };

export interface Repository<T> {
    findById(id: number): Promise<T | null>;
    findAll(): Promise<T[]>;
    create(item: Omit<T, 'id'>): Promise<T>;
    update(id: number, item: Partial<T>): Promise<T>;
    delete(id: number): Promise<void>;
}

export const DEFAULT_PAGE_SIZE = 25;
export const MAX_PAGE_SIZE = 100;
```

- [ ] **Step 6: Create fixtures/typescript/class_with_abstract.ts**

```typescript
export abstract class BaseService<T> {
    protected cache: Map<number, T> = new Map();

    abstract findById(id: number): Promise<T | null>;
    abstract create(data: Omit<T, 'id'>): Promise<T>;

    async findCached(id: number): Promise<T | null> {
        if (this.cache.has(id)) {
            return this.cache.get(id)!;
        }
        const result = await this.findById(id);
        if (result) {
            this.cache.set(id, result);
        }
        return result;
    }

    clearCache(): void {
        this.cache.clear();
        console.log('Cache cleared');
    }
}

export class UserServiceImpl extends BaseService<User> {
    constructor(private readonly db: Database) {
        super();
    }

    async findById(id: number): Promise<User | null> {
        const row = await this.db.query('SELECT * FROM users WHERE id = $1', [id]);
        if (!row) return null;
        return this.mapRow(row);
    }

    async create(data: Omit<User, 'id'>): Promise<User> {
        const row = await this.db.query(
            'INSERT INTO users (name, email, role) VALUES ($1, $2, $3) RETURNING *',
            [data.name, data.email, data.role]
        );
        return this.mapRow(row);
    }

    private mapRow(row: any): User {
        return {
            id: row.id,
            name: row.name,
            email: row.email,
            role: row.role,
        };
    }
}
```

- [ ] **Step 7: Create fixtures/typescript/arrow_functions.ts**

```typescript
import { User, Result } from './types';

// Expression-bodied (should NOT be pruned)
export const isAdmin = (user: User): boolean => user.role === 'admin';
export const getUserName = (user: User): string => user.name;

// Block-bodied (should be pruned)
export const validateUser = (user: User): Result<User> => {
    if (!user.name || user.name.trim() === '') {
        return { ok: false, error: new Error('Name is required') };
    }
    if (!user.email || !user.email.includes('@')) {
        return { ok: false, error: new Error('Valid email is required') };
    }
    return { ok: true, value: user };
};

export const fetchUsers = async (page: number = 1): Promise<User[]> => {
    const response = await fetch(`/api/users?page=${page}`);
    if (!response.ok) {
        throw new Error(`Failed to fetch users: ${response.status}`);
    }
    const data = await response.json();
    return data.users;
};
```

- [ ] **Step 8: Commit fixtures**

```bash
git add fixtures/javascript/ fixtures/typescript/
git commit -m "test(fixtures): add JavaScript and TypeScript test fixtures"
```

---

### Task 14: Implement Shared JS/TS Logic and JavaScriptBackend

**Files:**
- Create: `crates/skltn-core/src/backend/js_common.rs`
- Create: `crates/skltn-core/src/backend/javascript.rs`
- Modify: `crates/skltn-core/src/backend/mod.rs`

- [ ] **Step 1: Write js_common.rs — shared logic**

```rust
use tree_sitter::Node;

/// Shared logic for JavaScript and TypeScript backends.
/// TS is a superset of JS, so most structural node classification is identical.

/// Is this node a structural node shared by both JS and TS?
pub fn is_structural_node_common(node: &Node) -> bool {
    matches!(
        node.kind(),
        "function_declaration"
            | "generator_function_declaration"
            | "method_definition"
            | "class_declaration"
            | "arrow_function"
            | "function"  // function expressions
            | "generator_function" // generator function expressions
    )
}

/// Is this a JSDoc-style doc comment?
pub fn is_doc_comment_common(node: &Node, source: &[u8]) -> bool {
    node.kind() == "comment"
        && node
            .utf8_text(source)
            .map(|t| t.starts_with("/**"))
            .unwrap_or(false)
}

/// Find the body node for a JS/TS structural node.
pub fn body_node_common<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    match node.kind() {
        // Leaf structural nodes
        "function_declaration"
        | "generator_function_declaration"
        | "method_definition"
        | "function"
        | "generator_function" => {
            node.child_by_field_name("body")
        }
        "arrow_function" => {
            // Only prune block-bodied arrows
            node.child_by_field_name("body")
                .filter(|body| body.kind() == "statement_block")
        }
        // Container structural node
        "class_declaration" => None,
        _ => None,
    }
}

/// Format the replacement for a brace-delimited body (JS/TS).
pub fn format_replacement_common(
    indent: &str,
    placeholder: &str,
    line_count: usize,
    hidden_line_tag: &str,
) -> String {
    // For JS/TS, replace the entire statement_block node: { placeholder // tag }
    let inner_indent = format!("{}    ", indent);
    format!(
        "{{\n{}{} {}\n{}}}",
        inner_indent,
        placeholder,
        hidden_line_tag,
        indent,
    )
}
```

- [ ] **Step 2: Write javascript.rs**

```rust
use tree_sitter::{Language, Node};

use super::js_common;
use super::LanguageBackend;

pub struct JavaScriptBackend;

impl LanguageBackend for JavaScriptBackend {
    fn language(&self) -> Language {
        tree_sitter_javascript::LANGUAGE.into()
    }

    fn extensions(&self) -> &[&str] {
        &["js"]
    }

    fn is_structural_node(&self, node: &Node) -> bool {
        js_common::is_structural_node_common(node)
    }

    fn is_doc_comment(&self, node: &Node, source: &[u8]) -> bool {
        js_common::is_doc_comment_common(node, source)
    }

    fn body_node<'a>(&self, node: &Node<'a>) -> Option<Node<'a>> {
        js_common::body_node_common(node)
    }

    fn placeholder(&self) -> &str {
        "throw new Error(\"not implemented\")"
    }

    fn hidden_line_tag(&self, count: usize) -> String {
        format!("// [skltn: {} lines hidden]", count)
    }

    fn format_replacement(&self, indent: &str, line_count: usize, _body: &Node, _source: &[u8]) -> String {
        js_common::format_replacement_common(indent, self.placeholder(), line_count, &self.hidden_line_tag(line_count))
    }
}
```

- [ ] **Step 3: Update backend/mod.rs**

Add:
```rust
pub mod js_common;
pub mod javascript;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p skltn-core`
Expected: Successful compilation.

- [ ] **Step 5: Commit**

```bash
git add crates/skltn-core/src/backend/
git commit -m "feat(core): implement JavaScriptBackend with shared js_common module"
```

---

### Task 15: Implement TypeScriptBackend

**Files:**
- Create: `crates/skltn-core/src/backend/typescript.rs`
- Modify: `crates/skltn-core/src/backend/mod.rs`

- [ ] **Step 1: Write typescript.rs**

```rust
use tree_sitter::{Language, Node};

use super::js_common;
use super::LanguageBackend;

pub struct TypeScriptBackend;

impl LanguageBackend for TypeScriptBackend {
    fn language(&self) -> Language {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }

    fn extensions(&self) -> &[&str] {
        &["ts"]
    }

    fn is_structural_node(&self, node: &Node) -> bool {
        // All JS structural nodes + TS-specific abstract classes
        js_common::is_structural_node_common(node)
            || matches!(node.kind(), "abstract_class_declaration")
    }

    fn is_doc_comment(&self, node: &Node, source: &[u8]) -> bool {
        js_common::is_doc_comment_common(node, source)
    }

    fn body_node<'a>(&self, node: &Node<'a>) -> Option<Node<'a>> {
        match node.kind() {
            // TS-specific: abstract classes are containers
            "abstract_class_declaration" => None,
            // Delegate everything else to shared JS logic
            _ => js_common::body_node_common(node),
        }
    }

    fn placeholder(&self) -> &str {
        "throw new Error(\"not implemented\")"
    }

    fn hidden_line_tag(&self, count: usize) -> String {
        format!("// [skltn: {} lines hidden]", count)
    }

    fn format_replacement(&self, indent: &str, line_count: usize, _body: &Node, _source: &[u8]) -> String {
        js_common::format_replacement_common(indent, self.placeholder(), line_count, &self.hidden_line_tag(line_count))
    }
}
```

- [ ] **Step 2: Update backend/mod.rs**

Add `pub mod typescript;` to `crates/skltn-core/src/backend/mod.rs`.

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p skltn-core`
Expected: Successful compilation.

- [ ] **Step 4: Commit**

```bash
git add crates/skltn-core/src/backend/typescript.rs crates/skltn-core/src/backend/mod.rs
git commit -m "feat(core): implement TypeScriptBackend extending js_common with TS extras"
```

---

### Task 16: Write Snapshot Tests for JS and TS Backends

**Files:**
- Create: `crates/skltn-core/tests/javascript_backend.rs`
- Create: `crates/skltn-core/tests/typescript_backend.rs`

- [ ] **Step 1: Write JavaScript snapshot tests**

```rust
mod common;

use common::{assert_valid_syntax, default_opts};
use skltn_core::backend::javascript::JavaScriptBackend;
use skltn_core::engine::SkeletonEngine;

#[test]
fn test_js_simple_function() {
    let source = include_str!("../../../fixtures/javascript/simple_function.js");
    let backend = JavaScriptBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_js_class_with_methods() {
    let source = include_str!("../../../fixtures/javascript/class_with_methods.js");
    let backend = JavaScriptBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_js_arrow_functions() {
    let source = include_str!("../../../fixtures/javascript/arrow_functions.js");
    let backend = JavaScriptBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_js_simple_function_valid_syntax() {
    let source = include_str!("../../../fixtures/javascript/simple_function.js");
    let backend = JavaScriptBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    assert_valid_syntax(&result, &backend);
}

#[test]
fn test_js_arrow_functions_valid_syntax() {
    let source = include_str!("../../../fixtures/javascript/arrow_functions.js");
    let backend = JavaScriptBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    assert_valid_syntax(&result, &backend);
}
```

- [ ] **Step 2: Write TypeScript snapshot tests**

```rust
mod common;

use common::{assert_valid_syntax, default_opts};
use skltn_core::backend::typescript::TypeScriptBackend;
use skltn_core::engine::SkeletonEngine;

#[test]
fn test_ts_simple_function() {
    let source = include_str!("../../../fixtures/typescript/simple_function.ts");
    let backend = TypeScriptBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_ts_interface_and_types() {
    let source = include_str!("../../../fixtures/typescript/interface_and_types.ts");
    let backend = TypeScriptBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_ts_class_with_abstract() {
    let source = include_str!("../../../fixtures/typescript/class_with_abstract.ts");
    let backend = TypeScriptBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_ts_arrow_functions() {
    let source = include_str!("../../../fixtures/typescript/arrow_functions.ts");
    let backend = TypeScriptBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_ts_simple_function_valid_syntax() {
    let source = include_str!("../../../fixtures/typescript/simple_function.ts");
    let backend = TypeScriptBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    assert_valid_syntax(&result, &backend);
}

#[test]
fn test_ts_interface_and_types_unchanged() {
    // Interfaces and type aliases should pass through completely unchanged
    let source = include_str!("../../../fixtures/typescript/interface_and_types.ts");
    let backend = TypeScriptBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    assert_eq!(source, result, "Interfaces/types file should be unchanged after skeletonization");
}

#[test]
fn test_ts_class_with_abstract_valid_syntax() {
    let source = include_str!("../../../fixtures/typescript/class_with_abstract.ts");
    let backend = TypeScriptBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    assert_valid_syntax(&result, &backend);
}
```

- [ ] **Step 3: Run tests, review and accept snapshots**

Run: `cargo test -p skltn-core`
Then: `cargo insta review`
Expected: JS/TS skeletons show correct behavior — expression arrows unchanged, block arrows pruned, interfaces unchanged, abstract methods unchanged, regular methods pruned.

- [ ] **Step 4: Run tests to confirm all pass**

Run: `cargo test -p skltn-core`
Expected: ALL tests pass (Rust + Python + JS + TS).

- [ ] **Step 5: Commit**

```bash
git add crates/skltn-core/tests/
git commit -m "test(core): add JavaScript and TypeScript snapshot tests with round-trip validation"
```

---

## Chunk 5: CLI Implementation

### Task 17: Implement Backend Registry

**Files:**
- Modify: `crates/skltn-core/src/backend/mod.rs`

- [ ] **Step 1: Add registry function to backend/mod.rs**

Add the following to `crates/skltn-core/src/backend/mod.rs`:

```rust
pub mod rust;
pub mod python;
pub mod js_common;
pub mod javascript;
pub mod typescript;

use self::rust::RustBackend;
use self::python::PythonBackend;
use self::javascript::JavaScriptBackend;
use self::typescript::TypeScriptBackend;

/// Returns the appropriate backend for a file extension.
pub fn backend_for_extension(ext: &str) -> Option<Box<dyn LanguageBackend>> {
    match ext {
        "rs" => Some(Box::new(RustBackend)),
        "py" => Some(Box::new(PythonBackend)),
        "ts" => Some(Box::new(TypeScriptBackend)),
        "js" => Some(Box::new(JavaScriptBackend)),
        _ => None,
    }
}

/// Returns the appropriate backend for a language name string.
pub fn backend_for_lang(lang: &str) -> Option<Box<dyn LanguageBackend>> {
    match lang {
        "rust" | "rs" => Some(Box::new(RustBackend)),
        "python" | "py" => Some(Box::new(PythonBackend)),
        "typescript" | "ts" => Some(Box::new(TypeScriptBackend)),
        "javascript" | "js" => Some(Box::new(JavaScriptBackend)),
        _ => None,
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p skltn-core`
Expected: Successful compilation.

- [ ] **Step 3: Commit**

```bash
git add crates/skltn-core/src/backend/mod.rs
git commit -m "feat(core): add backend registry functions for extension and language lookup"
```

---

### Task 18: Implement the CLI

**Files:**
- Modify: `crates/skltn-cli/src/main.rs`

- [ ] **Step 1: Write the CLI**

```rust
use std::io::{self, Write};
use std::path::PathBuf;
use std::process;

use clap::Parser;
use ignore::WalkBuilder;
use is_terminal::IsTerminal;

use skltn_core::backend::{backend_for_extension, backend_for_lang};
use skltn_core::engine::SkeletonEngine;
use skltn_core::options::SkeletonOptions;

#[derive(Parser)]
#[command(name = "skltn", version, about = "Skeletonize source code for AI context compression")]
struct Cli {
    /// File or directory to skeletonize
    path: PathBuf,

    /// Maximum nesting depth (default: unlimited)
    #[arg(long)]
    max_depth: Option<usize>,

    /// Force language detection (rust, python, typescript, javascript)
    #[arg(long)]
    lang: Option<String>,

    /// Output without markdown fencing
    #[arg(long)]
    raw: bool,
}

fn main() {
    let cli = Cli::parse();
    let options = SkeletonOptions {
        max_depth: cli.max_depth,
    };

    let is_tty = io::stdout().is_terminal();
    let use_markdown = !cli.raw && is_tty;

    if cli.path.is_file() {
        process_file(&cli.path, &options, cli.lang.as_deref(), use_markdown);
    } else if cli.path.is_dir() {
        process_directory(&cli.path, &options, use_markdown);
    } else {
        eprintln!("Error: '{}' is not a valid file or directory", cli.path.display());
        process::exit(1);
    }
}

fn process_file(
    path: &PathBuf,
    options: &SkeletonOptions,
    lang_override: Option<&str>,
    use_markdown: bool,
) {
    let backend = if let Some(lang) = lang_override {
        match backend_for_lang(lang) {
            Some(b) => b,
            None => {
                eprintln!("Error: unsupported language '{}'", lang);
                process::exit(1);
            }
        }
    } else {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        match backend_for_extension(ext) {
            Some(b) => b,
            None => {
                eprintln!("Error: unsupported file extension '.{}'", ext);
                process::exit(1);
            }
        }
    };

    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{}': {}", path.display(), e);
            process::exit(1);
        }
    };

    let skeleton = match SkeletonEngine::skeletonize(&source, backend.as_ref(), options) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error skeletonizing '{}': {}", path.display(), e);
            process::exit(1);
        }
    };

    let stdout = io::stdout();
    let mut out = stdout.lock();

    if use_markdown {
        let lang_tag = path
            .extension()
            .and_then(|e| e.to_str())
            .map(ext_to_lang_tag)
            .unwrap_or("");
        writeln!(out, "```{}", lang_tag).unwrap();
        write!(out, "{}", skeleton).unwrap();
        writeln!(out, "```").unwrap();
    } else {
        write!(out, "{}", skeleton).unwrap();
    }
}

fn process_directory(dir: &PathBuf, options: &SkeletonOptions, use_markdown: bool) {
    let mut found_any = false;

    let walker = WalkBuilder::new(dir)
        .standard_filters(true) // respects .gitignore
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e,
            None => continue,
        };

        let backend = match backend_for_extension(ext) {
            Some(b) => b,
            None => continue,
        };

        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: could not read '{}': {}", path.display(), e);
                continue;
            }
        };

        let skeleton = match SkeletonEngine::skeletonize(&source, backend.as_ref(), options) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: skeletonize failed for '{}': {}", path.display(), e);
                continue;
            }
        };

        found_any = true;
        let relative = path.strip_prefix(dir).unwrap_or(path);
        let stdout = io::stdout();
        let mut out = stdout.lock();

        if use_markdown {
            let lang_tag = ext_to_lang_tag(ext);
            writeln!(out, "## File: {}", relative.display()).unwrap();
            writeln!(out, "```{}", lang_tag).unwrap();
            write!(out, "{}", skeleton).unwrap();
            writeln!(out, "```").unwrap();
            writeln!(out).unwrap();
        } else {
            writeln!(out, "// === {} ===", relative.display()).unwrap();
            write!(out, "{}", skeleton).unwrap();
            writeln!(out).unwrap();
        }
    }

    if !found_any {
        eprintln!(
            "Warning: no supported files found in '{}'",
            dir.display()
        );
    }
}

fn ext_to_lang_tag(ext: &str) -> &str {
    match ext {
        "rs" => "rust",
        "py" => "python",
        "ts" => "typescript",
        "js" => "javascript",
        _ => ext,
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p skltn-cli`
Expected: Successful compilation.

- [ ] **Step 3: Manual smoke test — single file**

Run: `cargo run -p skltn-cli -- fixtures/rust/simple_function.rs`
Expected: Skeleton output with `todo!() // [skltn: N lines hidden]` for function bodies, imports preserved.

- [ ] **Step 4: Manual smoke test — directory**

Run: `cargo run -p skltn-cli -- fixtures/rust/`
Expected: Multiple files output with `## File:` headers and markdown fences.

- [ ] **Step 5: Manual smoke test — raw mode**

Run: `cargo run -p skltn-cli -- --raw fixtures/python/class_with_methods.py`
Expected: Skeleton output without markdown fencing. Functions show `pass # [skltn: N lines hidden]`.

- [ ] **Step 6: Commit**

```bash
git add crates/skltn-cli/src/main.rs
git commit -m "feat(cli): implement skltn CLI with clap, ignore traversal, and TTY-aware output"
```

---

## Chunk 6: Edge Case Fixtures and Final Validation

### Task 19: Create Remaining Edge Case Fixtures

**Files:**
- Create: `fixtures/rust/closures.rs`
- Create: `fixtures/rust/cfg_test_module.rs`
- Create: `fixtures/rust/constants_only.rs`
- Create: `fixtures/rust/nested_impl_blocks.rs`
- Create: `fixtures/rust/syntax_error.rs`
- Create: `fixtures/python/decorators.py`
- Create: `fixtures/python/nested_classes.py`
- Create: `fixtures/python/lambdas.py`
- Create: `fixtures/python/syntax_error.py`
- Create: `fixtures/javascript/es_module_exports.js`
- Create: `fixtures/javascript/decorators.js`
- Create: `fixtures/typescript/overloads.ts`
- Create: `fixtures/typescript/decorators.ts`

- [ ] **Step 1: Create fixtures/rust/closures.rs**

```rust
use std::collections::HashMap;

pub fn process_items(items: &[String]) -> Vec<String> {
    // Block-bodied closure — should be pruned
    let transform = |s: &String| {
        let trimmed = s.trim().to_lowercase();
        let mut result = String::with_capacity(trimmed.len() + 10);
        result.push_str("processed_");
        result.push_str(&trimmed);
        result
    };

    items.iter().map(transform).collect()
}

pub fn create_handler() -> impl Fn(i32) -> i32 {
    // Expression closure — should NOT be pruned
    |x| x * 2
}

pub fn sort_by_key(items: &mut [(String, i32)]) {
    // Short expression closure in method chain — should NOT be pruned
    items.sort_by(|a, b| a.1.cmp(&b.1));
}
```

- [ ] **Step 2: Create fixtures/rust/cfg_test_module.rs**

```rust
pub struct Calculator;

impl Calculator {
    pub fn add(&self, a: i32, b: i32) -> i32 {
        let result = a + b;
        result
    }

    pub fn multiply(&self, a: i32, b: i32) -> i32 {
        a.checked_mul(b).unwrap_or(i32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        let calc = Calculator;
        assert_eq!(calc.add(2, 3), 5);
        assert_eq!(calc.add(-1, 1), 0);
        assert_eq!(calc.add(0, 0), 0);
    }

    #[test]
    fn test_multiply() {
        let calc = Calculator;
        assert_eq!(calc.multiply(2, 3), 6);
        assert_eq!(calc.multiply(-1, 5), -5);
        assert_eq!(calc.multiply(i32::MAX, 2), i32::MAX);
    }
}
```

- [ ] **Step 3: Create fixtures/rust/constants_only.rs**

```rust
//! Configuration constants for the skeleton engine.

pub const MAX_FILE_SIZE: usize = 1_048_576;
pub const DEFAULT_MAX_DEPTH: usize = 100;
pub const SUPPORTED_EXTENSIONS: &[&str] = &["rs", "py", "ts", "js"];
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The default tag prefix used in skeleton output.
pub const TAG_PREFIX: &str = "skltn";
```

- [ ] **Step 4: Create fixtures/rust/nested_impl_blocks.rs**

```rust
pub trait Processor {
    fn process(&self, input: &str) -> String;
}

pub struct TextProcessor {
    pub prefix: String,
}

impl TextProcessor {
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
        }
    }

    pub fn with_suffix(&self, input: &str, suffix: &str) -> String {
        let mut result = self.process(input);
        result.push_str(suffix);
        result
    }
}

impl Processor for TextProcessor {
    fn process(&self, input: &str) -> String {
        let trimmed = input.trim();
        let result = format!("{}_{}", self.prefix, trimmed);
        result.to_uppercase()
    }
}
```

- [ ] **Step 5: Create fixtures/rust/syntax_error.rs**

```rust
pub fn valid_function(x: i32) -> i32 {
    x + 1
}

pub fn another_valid(s: &str) -> String {
    s.to_uppercase()
}

// This line has a syntax error
pub fn broken(x: i32 -> {
    x
}

pub fn after_error(y: i32) -> i32 {
    y * 2
}
```

- [ ] **Step 6: Create fixtures/python/decorators.py**

```python
import functools
import time


def timer(func):
    """Decorator that times function execution."""
    @functools.wraps(func)
    def wrapper(*args, **kwargs):
        start = time.perf_counter()
        result = func(*args, **kwargs)
        elapsed = time.perf_counter() - start
        print(f"{func.__name__} took {elapsed:.4f}s")
        return result
    return wrapper


def retry(max_attempts: int = 3):
    """Decorator factory for retry logic."""
    def decorator(func):
        @functools.wraps(func)
        def wrapper(*args, **kwargs):
            for attempt in range(max_attempts):
                try:
                    return func(*args, **kwargs)
                except Exception as e:
                    if attempt == max_attempts - 1:
                        raise
                    time.sleep(2 ** attempt)
        return wrapper
    return decorator


@timer
def slow_operation(data: list) -> list:
    """A slow operation that benefits from timing."""
    result = []
    for item in data:
        processed = item.strip().lower()
        result.append(processed)
        time.sleep(0.01)
    return result


@retry(max_attempts=5)
def fetch_with_retry(url: str) -> dict:
    """Fetch data from URL with automatic retries."""
    import urllib.request
    response = urllib.request.urlopen(url)
    return json.loads(response.read())
```

- [ ] **Step 7: Create fixtures/python/nested_classes.py**

```python
class Outer:
    """Outer class with nested classes."""

    class Inner:
        """Inner configuration class."""
        value: int = 42

        def get_value(self) -> int:
            """Return the configured value."""
            return self.value * 2

    class AnotherInner:
        """Another nested class."""

        def compute(self, x: int) -> int:
            """Perform computation."""
            result = x ** 2
            if result > 1000:
                result = 1000
            return result

    def use_inner(self) -> int:
        """Use the inner class."""
        inner = self.Inner()
        return inner.get_value()
```

- [ ] **Step 8: Create fixtures/python/lambdas.py**

```python
from typing import Callable

# Lambdas should be emitted verbatim
double = lambda x: x * 2
greet = lambda name: f"Hello, {name}!"

TRANSFORMS: dict[str, Callable] = {
    "upper": lambda s: s.upper(),
    "lower": lambda s: s.lower(),
    "strip": lambda s: s.strip(),
}


def apply_transforms(data: list[str], transform_name: str) -> list[str]:
    """Apply a named transform to all items."""
    transform = TRANSFORMS.get(transform_name)
    if transform is None:
        raise ValueError(f"Unknown transform: {transform_name}")
    return [transform(item) for item in data]


def sort_by_length(items: list[str]) -> list[str]:
    """Sort items by string length."""
    return sorted(items, key=lambda s: len(s))
```

- [ ] **Step 9: Create fixtures/python/syntax_error.py**

```python
def valid_function(x: int) -> int:
    """A valid function."""
    return x + 1


def another_valid(s: str) -> str:
    return s.upper()


# This has a syntax error
def broken(x: int ->:
    return x


def after_error(y: int) -> int:
    return y * 2
```

- [ ] **Step 10: Create remaining JS/TS fixtures**

`fixtures/javascript/es_module_exports.js`:
```javascript
export const API_VERSION = '2.0';
export const BASE_URL = 'https://api.example.com';

export function createClient(apiKey) {
    const headers = {
        'Authorization': `Bearer ${apiKey}`,
        'Content-Type': 'application/json',
        'X-API-Version': API_VERSION,
    };
    return {
        get: async (path) => {
            const res = await fetch(`${BASE_URL}${path}`, { headers });
            return res.json();
        },
        post: async (path, body) => {
            const res = await fetch(`${BASE_URL}${path}`, {
                method: 'POST',
                headers,
                body: JSON.stringify(body),
            });
            return res.json();
        },
    };
}

export default createClient;
```

`fixtures/javascript/decorators.js`:
```javascript
function log(target, name, descriptor) {
    const original = descriptor.value;
    descriptor.value = function (...args) {
        console.log(`Calling ${name} with`, args);
        const result = original.apply(this, args);
        console.log(`${name} returned`, result);
        return result;
    };
    return descriptor;
}

class TaskManager {
    constructor() {
        this.tasks = [];
    }

    addTask(task) {
        this.tasks.push({
            ...task,
            createdAt: Date.now(),
            status: 'pending',
        });
        return this.tasks.length - 1;
    }

    completeTask(index) {
        if (index < 0 || index >= this.tasks.length) {
            throw new RangeError('Task index out of bounds');
        }
        this.tasks[index].status = 'completed';
        this.tasks[index].completedAt = Date.now();
    }
}

export { TaskManager };
```

`fixtures/typescript/overloads.ts`:
```typescript
// Function overloads — signatures should be preserved, implementation pruned
export function format(value: string): string;
export function format(value: number): string;
export function format(value: Date): string;
export function format(value: string | number | Date): string {
    if (typeof value === 'string') {
        return value.trim();
    }
    if (typeof value === 'number') {
        return value.toFixed(2);
    }
    return value.toISOString();
}

export function parse(input: string, type: 'number'): number;
export function parse(input: string, type: 'boolean'): boolean;
export function parse(input: string, type: 'number' | 'boolean'): number | boolean {
    if (type === 'number') {
        const num = Number(input);
        if (isNaN(num)) throw new Error('Invalid number');
        return num;
    }
    return input === 'true';
}
```

`fixtures/typescript/decorators.ts`:
```typescript
function Injectable() {
    return function (target: any) {
        Reflect.defineMetadata('injectable', true, target);
    };
}

function Log(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function (...args: any[]) {
        console.log(`[${propertyKey}] called with:`, args);
        const result = original.apply(this, args);
        console.log(`[${propertyKey}] returned:`, result);
        return result;
    };
    return descriptor;
}

@Injectable()
export class OrderService {
    private orders: Map<string, Order> = new Map();

    @Log
    createOrder(items: OrderItem[]): Order {
        const order: Order = {
            id: crypto.randomUUID(),
            items,
            total: items.reduce((sum, item) => sum + item.price * item.quantity, 0),
            createdAt: new Date(),
        };
        this.orders.set(order.id, order);
        return order;
    }

    @Log
    cancelOrder(orderId: string): boolean {
        const order = this.orders.get(orderId);
        if (!order) return false;
        this.orders.delete(orderId);
        return true;
    }
}
```

- [ ] **Step 11: Commit all edge case fixtures**

```bash
git add fixtures/
git commit -m "test(fixtures): add edge case fixtures for closures, decorators, lambdas, syntax errors, and more"
```

---

### Task 20: Write Edge Case Tests

**Files:**
- Create: `crates/skltn-core/tests/edge_cases.rs`

- [ ] **Step 1: Write edge case tests**

```rust
mod common;

use common::{assert_valid_syntax, default_opts, has_error_nodes};
use skltn_core::backend::javascript::JavaScriptBackend;
use skltn_core::backend::python::PythonBackend;
use skltn_core::backend::rust::RustBackend;
use skltn_core::backend::typescript::TypeScriptBackend;
use skltn_core::backend::LanguageBackend;
use skltn_core::engine::SkeletonEngine;
use skltn_core::options::SkeletonOptions;

// --- Rust edge cases ---

#[test]
fn test_rust_closures() {
    let source = include_str!("../../../fixtures/rust/closures.rs");
    let backend = RustBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_rust_cfg_test_module() {
    let source = include_str!("../../../fixtures/rust/cfg_test_module.rs");
    let backend = RustBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_rust_constants_only() {
    let source = include_str!("../../../fixtures/rust/constants_only.rs");
    let backend = RustBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    // File with no structural nodes should be unchanged
    assert_eq!(source, result, "Constants-only file should be unchanged");
}

#[test]
fn test_rust_nested_impl_blocks() {
    let source = include_str!("../../../fixtures/rust/nested_impl_blocks.rs");
    let backend = RustBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_rust_syntax_error() {
    let source = include_str!("../../../fixtures/rust/syntax_error.rs");
    let backend = RustBackend;
    // Should not panic — partial parse via tree-sitter error tolerance
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

// --- Python edge cases ---

#[test]
fn test_python_decorators() {
    let source = include_str!("../../../fixtures/python/decorators.py");
    let backend = PythonBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_python_nested_classes() {
    let source = include_str!("../../../fixtures/python/nested_classes.py");
    let backend = PythonBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_python_lambdas() {
    let source = include_str!("../../../fixtures/python/lambdas.py");
    let backend = PythonBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_python_syntax_error() {
    let source = include_str!("../../../fixtures/python/syntax_error.py");
    let backend = PythonBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

// --- JS/TS edge cases ---

#[test]
fn test_js_es_module_exports() {
    let source = include_str!("../../../fixtures/javascript/es_module_exports.js");
    let backend = JavaScriptBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_js_decorators() {
    let source = include_str!("../../../fixtures/javascript/decorators.js");
    let backend = JavaScriptBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_ts_overloads() {
    let source = include_str!("../../../fixtures/typescript/overloads.ts");
    let backend = TypeScriptBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn test_ts_decorators() {
    let source = include_str!("../../../fixtures/typescript/decorators.ts");
    let backend = TypeScriptBackend;
    let result = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();
    insta::assert_snapshot!(result);
}

// --- Depth limiting ---

#[test]
fn test_rust_depth_limit_1() {
    let source = include_str!("../../../fixtures/rust/nested_impl_blocks.rs");
    let backend = RustBackend;
    let opts = SkeletonOptions { max_depth: Some(1) };
    let result = SkeletonEngine::skeletonize(source, &backend, &opts).unwrap();
    insta::assert_snapshot!(result);
}

// --- CRLF handling ---

#[test]
fn test_crlf_handling() {
    let source = include_str!("../../../fixtures/rust/simple_function.rs");
    let crlf_source = source.replace('\n', "\r\n");
    let backend = RustBackend;
    let result = SkeletonEngine::skeletonize(&crlf_source, &backend, &default_opts()).unwrap();
    // Should produce valid output (may have \r\n or \n, but no parse errors)
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&backend.language()).unwrap();
    let tree = parser.parse(&result, None).unwrap();
    assert!(
        !has_error_nodes(&tree.root_node()),
        "CRLF skeleton has syntax errors:\n{}",
        result
    );
}
```

- [ ] **Step 2: Run all tests, review and accept new snapshots**

Run: `cargo test -p skltn-core`
Then: `cargo insta review`
Expected: All new snapshots look correct. Closures, decorators, lambdas, syntax errors, depth limiting all behave as specified.

- [ ] **Step 3: Run full test suite to confirm everything passes**

Run: `cargo test`
Expected: ALL tests pass across both crates.

- [ ] **Step 4: Commit**

```bash
git add crates/skltn-core/tests/
git commit -m "test(core): add edge case tests for closures, decorators, lambdas, depth limits, CRLF, and syntax errors"
```

---

### Task 21: Final Validation — Run CLI Against Own Codebase

- [ ] **Step 1: Skeletonize the skltn project itself**

Run: `cargo run -p skltn-cli -- crates/skltn-core/src/`
Expected: Markdown-fenced output of all `skltn-core` source files, with function bodies pruned and structs/traits/enums preserved in full. This is the dogfooding test.

- [ ] **Step 2: Skeletonize the fixtures directory**

Run: `cargo run -p skltn-cli -- fixtures/`
Expected: All fixture files across all 4 languages are skeletonized correctly.

- [ ] **Step 3: Verify raw mode works with piping**

Run: `cargo run -p skltn-cli -- --raw fixtures/rust/simple_function.rs | wc -l`
Expected: Line count is significantly less than the original file.

- [ ] **Step 4: Verify error handling — unsupported file**

Run: `cargo run -p skltn-cli -- Cargo.toml 2>&1`
Expected: Error message to stderr about unsupported extension `.toml`, exit code 1.

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "feat: Phase 1 Skeleton Engine complete — library + CLI with 4 language backends"
```
