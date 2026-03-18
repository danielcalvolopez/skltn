# Phase 1: Skeleton Engine — Design Specification

**Project:** skltn (Skeleton)
**Phase:** 1 of 4
**Date:** 2026-03-16
**Status:** Approved

---

## Overview

The Skeleton Engine is a Rust library and CLI tool that uses tree-sitter AST parsing to compress source code into architectural skeletons. It strips function/method bodies while preserving signatures, type definitions, doc comments, and imports — reducing token count by 75%+ while maintaining syntactic validity.

This is Phase 1 of the skltn project. Each phase ships a standalone, testable deliverable before the next phase begins.

---

## Guiding Principles

1. **Each phase is independently testable.** Phase 1 produces a working CLI that can be validated against real codebases before MCP integration begins.
2. **Syntactic validity above all.** Skeleton output must parse without errors in the original language. AI models reason better about syntactically valid code.
3. **Structural Nodes vs. Everything Else.** Functions and methods have their bodies pruned. Everything else (structs, enums, types, constants, imports) passes through verbatim.

---

## Supported Languages (Phase 1)

| Language   | Extensions | tree-sitter Grammar         | Placeholder      |
|------------|------------|-----------------------------|------------------|
| Rust       | `.rs`      | `tree-sitter-rust`          | `todo!()`        |
| Python     | `.py`      | `tree-sitter-python`        | `pass`           |
| TypeScript | `.ts`      | `tree-sitter-typescript`    | `throw new Error("not implemented")` |
| JavaScript | `.js`      | `tree-sitter-javascript`    | `throw new Error("not implemented")` |

> **PRD Deviation:** The PRD specifies `.sol` (Solidity) as a Phase 1 language. This was replaced with JavaScript (`.js`) based on team decision — JS/TS coverage is more immediately useful for the target codebase, and JS shares grammar infrastructure with TS (reducing implementation cost). Solidity support will be added in a future phase as a standalone `SolidityBackend`.

---

## Architecture

### Cargo Workspace Structure

```
skltn/
├── Cargo.toml                  # Workspace root
├── crates/
│   ├── skltn-core/             # Library crate — the Skeleton Engine
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs          # Public API: skeletonize(source, lang, opts)
│   │       ├── engine.rs       # SkeletonEngine — AST walker + folding logic
│   │       ├── backend/
│   │       │   ├── mod.rs      # LanguageBackend trait definition
│   │       │   ├── rust.rs     # RustBackend
│   │       │   ├── python.rs   # PythonBackend
│   │       │   ├── js_common.rs # Shared JS/TS structural node logic
│   │       │   ├── typescript.rs # TypeScriptBackend (delegates to js_common + TS extras)
│   │       │   └── javascript.rs # JavaScriptBackend (delegates to js_common)
│   │       ├── options.rs      # SkeletonOptions
│   │       └── error.rs        # Error types
│   └── skltn-cli/              # Binary crate — the CLI
│       ├── Cargo.toml
│       └── src/
│           └── main.rs         # clap args, ignore traversal, stdout output
├── fixtures/                   # Test fixture files (per language)
│   ├── rust/
│   ├── python/
│   ├── typescript/
│   └── javascript/
└── snapshots/                  # insta snapshot outputs
```

Phase 2's MCP server will be a third crate (`skltn-mcp`) in this workspace, depending on `skltn-core`.

### Key Dependencies

| Crate         | Purpose                                      |
|---------------|----------------------------------------------|
| `tree-sitter` | AST parsing                                  |
| `tree-sitter-rust`, `tree-sitter-python`, `tree-sitter-typescript`, `tree-sitter-javascript` | Language grammars |
| `clap`        | CLI argument parsing                         |
| `ignore`      | `.gitignore`-aware filesystem traversal      |
| `is-terminal` | TTY detection for output formatting          |
| `insta`       | Snapshot testing                             |

---

## Core Design: The `LanguageBackend` Trait

Each supported language implements this trait. The engine delegates all language-specific decisions to the backend.

```rust
pub trait LanguageBackend {
    /// Returns the tree-sitter Language grammar
    fn language(&self) -> tree_sitter::Language;

    /// File extensions this backend handles (e.g., ["rs"])
    fn extensions(&self) -> &[&str];

    /// Is this AST node a structural node?
    /// Two categories:
    /// - "Leaf" structural nodes (functions, methods, closures) → have a body to prune
    /// - "Container" structural nodes (impl blocks, classes, modules) → no body to prune,
    ///   but engine must recurse into their children
    /// The distinction is made by body_node(): containers return None, leaves return Some.
    fn is_structural_node(&self, node: &Node) -> bool;

    /// Is this a doc comment node that should be preserved?
    /// Source bytes are required because tree-sitter node kinds alone can't
    /// distinguish doc comments (///, /**) from regular comments (//).
    fn is_doc_comment(&self, node: &Node, source: &[u8]) -> bool;

    /// Given a structural node, return the child node representing the body
    /// to be replaced.
    /// Returns None for:
    /// - Container structural nodes (impl blocks, classes, modules) — engine recurses into children
    /// - Abstract/interface methods — no body exists, emit verbatim
    fn body_node<'a>(&self, node: &'a Node<'a>) -> Option<Node<'a>>;

    /// Returns the idiomatic placeholder for this language
    /// e.g., "todo!()" for Rust, "pass" for Python
    fn placeholder(&self) -> &str;

    /// Returns the formatted line-count tag
    /// e.g., "// [skltn: 47 lines hidden]"
    fn hidden_line_tag(&self, count: usize) -> String;

    /// Format the replacement text for a pruned body.
    /// Language-specific: brace-delimited languages produce `{ placeholder }`,
    /// indentation-based languages (Python) produce indented placeholder.
    /// `body` and `source` allow extraction of leading docstrings (Python).
    fn format_replacement(&self, indent: &str, line_count: usize, body: &Node, source: &[u8]) -> String;
}
```

### Design Rationale

- **No `is_data_node()` method.** The engine's logic is binary: "Is it structural? Prune the body. Everything else? Emit verbatim." This avoids forcing backends to enumerate every possible data node type and eliminates the ambiguous "neither structural nor data" case.
- **Container vs. leaf structural nodes.** Both `impl` blocks and functions return `true` for `is_structural_node()`. The distinction lives in `body_node()`: containers (impl blocks, classes, modules) return `None` — the engine recurses into their children looking for leaf structural nodes. Leaf nodes (functions, methods) return `Some(body)` — the engine prunes the body. This keeps the trait simple (one method for identification, one for body extraction) while correctly handling nesting.
- **JS/TS shared logic.** TypeScript is a superset of JavaScript. A `js_common.rs` module contains the shared structural node identification logic. `JavaScriptBackend` delegates to it directly. `TypeScriptBackend` delegates to it and adds TS-specific handling (interfaces, type aliases, abstract methods).

---

## Core Design: The Skeleton Engine

### Algorithm

The engine is stateless. It takes source code, a backend, and options, and returns the skeletonized output.

```rust
pub struct SkeletonEngine;

impl SkeletonEngine {
    pub fn skeletonize(
        source: &str,
        backend: &dyn LanguageBackend,
        options: &SkeletonOptions,
    ) -> Result<String, SkltnError>;
}

pub struct SkeletonOptions {
    pub max_depth: Option<usize>,  // None = unlimited
}
```

### AST Walking Logic

1. Parse source code into a tree-sitter AST.
2. Walk the AST depth-first, tracking current nesting depth of leaf structural nodes.
3. For each node:
   - **Doc comment** (`backend.is_doc_comment()`) → preserve in output.
   - **Structural node** (`backend.is_structural_node()`):
     - Call `backend.body_node(node)`:
       - `Some(body)` → **leaf structural node** (function, method):
         - If `current_depth >= max_depth` → emit verbatim, do not recurse.
         - Otherwise → register a replacement: replace the body's inner content with `placeholder + " " + hidden_line_tag(line_count)`. Surrounding braces/delimiters are preserved; only the content inside is replaced.
         - Increment depth counter, recurse into children (for nested functions), decrement on return.
       - `None` → **container structural node** (impl block, class, module) or abstract method:
         - Recurse into children to find nested leaf structural nodes. Do not increment depth counter.
   - **Everything else** (imports, data types, constants, module declarations) → emit verbatim. Skip subtree (no recursion needed — data nodes don't contain structural children).
   - **ERROR node** (tree-sitter parse failure) → emit verbatim with warning comment using language-appropriate syntax (e.g., `// [skltn: parse error, emitting raw]` for Rust/TS/JS, `# [skltn: parse error, emitting raw]` for Python).

### Node Classification Examples

| Node Type | Language | `is_structural_node()` | `body_node()` | Behavior |
|---|---|---|---|---|
| `function_item` | Rust | `true` | `Some(block)` | Prune body |
| `impl_item` | Rust | `true` | `None` | Recurse into children |
| `struct_item` | Rust | `false` | N/A | Emit verbatim, skip subtree |
| `function_definition` | Python | `true` | `Some(block)` | Prune body |
| `class_definition` | Python | `true` | `None` | Recurse into children |
| `function_declaration` | TS/JS | `true` | `Some(statement_block)` | Prune body |
| `class_declaration` | TS/JS | `true` | `None` | Recurse into children |
| `interface_declaration` | TS | `false` | N/A | Emit verbatim, skip subtree |
| `abstract_method` | TS | `true` | `None` | Emit verbatim (no body) |

### Closure and Lambda Handling

- **Rust closures** (`closure_expression`): Treated as leaf structural nodes. `body_node()` returns the closure body. Short closures (single expression, no block) are emitted verbatim — only block-bodied closures are pruned.
- **JS/TS arrow functions** (`arrow_function`): Block-bodied arrows (`=> { ... }`) are pruned. Expression-bodied arrows (`=> expr`) are emitted verbatim — they have no block to replace.
- **Python lambdas** (`lambda`): Emitted verbatim. Lambdas are single expressions by definition and cannot be pruned.

### Rust `#[cfg(test)]` Modules

Test modules (`mod tests { ... }` annotated with `#[cfg(test)]`) are treated as container structural nodes. The engine recurses into them and prunes individual test function bodies, same as any other module. The module declaration, attributes, and structure remain visible in the skeleton.

### Byte-Range Replacement Strategy

The engine does not build output by concatenating AST node text. Instead:

1. Collect all replacements as `(start_byte, end_byte, replacement_text)` tuples.
2. Sort by `start_byte` descending (reverse order).
3. Apply replacements from end of file to beginning — this preserves byte offsets for subsequent replacements.
4. The result is the original source with surgical body replacements, preserving all original formatting and whitespace.

### Line Count Calculation

Hidden line count for the `[skltn: N lines hidden]` tag:

```
line_count = body_node.end_position().row - body_node.start_position().row + 1
```

### Indentation Handling

The engine reads the body node's start column from the AST to determine correct indentation for the placeholder. This is especially critical for Python where `pass` must be indented to the correct level.

---

## CLI Design

### Interface

```
skltn [OPTIONS] <PATH>

Arguments:
  <PATH>    File or directory to skeletonize

Options:
  --max-depth <N>     Maximum nesting depth (default: unlimited)
  --lang <LANG>       Force language detection override
  --raw               Output without markdown fencing
  -h, --help
  -V, --version
```

### Behavior

**Single file:**
- Detect language from file extension (or use `--lang` override).
- Skeletonize and print to stdout.
- If stdout is a TTY (detected via `is-terminal`), wrap in a markdown fence with language tag.
- If piping to another program, default to raw output.

**Directory:**
- Use `ignore` crate for recursive traversal (respects `.gitignore` automatically).
- Filter to supported file extensions.
- Skeletonize each file.
- Output as a markdown-fenced stream with file headers outside the fences:

````
## File: src/engine.rs
```rust
pub struct SkeletonEngine;

impl SkeletonEngine {
    pub fn skeletonize(source: &str, ...) -> Result<String, SkltnError> {
        todo!() // [skltn: 45 lines hidden]
    }
}
```

## File: scripts/analyze.py
```python
class Analyzer:
    """Runs token analysis."""
    def run(self, path: str) -> dict:
        pass  # [skltn: 112 lines hidden]
```
````

### Backend Resolution

Extension-to-backend mapping via a simple match:

```rust
fn backend_for_extension(ext: &str) -> Option<Box<dyn LanguageBackend>> {
    match ext {
        "rs" => Some(Box::new(RustBackend)),
        "py" => Some(Box::new(PythonBackend)),
        "ts" => Some(Box::new(TypeScriptBackend)),
        "js" => Some(Box::new(JavaScriptBackend)),
        _ => None,
    }
}
```

The `--lang` flag overrides extension-based detection.

### Error Handling

| Scenario | Behavior |
|---|---|
| Unsupported file extension (no `--lang`) | Error to stderr, exit code 1 |
| Directory with no supported files | Warning to stderr, exit code 0 |
| File with syntax errors | Partial skeleton with tree-sitter error tolerance, warning to stderr |

---

## Testing Strategy

### Framework

Snapshot testing with the `insta` crate. Each fixture is a real source file paired with a golden snapshot of its expected skeleton output.

### Fixture Files

```
fixtures/
├── rust/
│   ├── simple_function.rs
│   ├── struct_with_methods.rs
│   ├── nested_impl_blocks.rs
│   ├── enums_and_constants.rs
│   ├── doc_comments.rs
│   ├── closures.rs               # Block-bodied vs expression closures
│   ├── cfg_test_module.rs         # #[cfg(test)] mod tests { ... }
│   ├── constants_only.rs          # File with no structural nodes
│   └── syntax_error.rs
├── python/
│   ├── simple_function.py
│   ├── class_with_methods.py
│   ├── nested_classes.py
│   ├── decorators.py
│   ├── docstrings.py
│   ├── deeply_nested.py           # Indentation stress test
│   ├── lambdas.py                 # Lambda expressions (emitted verbatim)
│   └── syntax_error.py
├── typescript/
│   ├── simple_function.ts
│   ├── interface_and_types.ts
│   ├── class_with_abstract.ts
│   ├── overloads.ts
│   ├── arrow_functions.ts         # Block-bodied and expression-bodied
│   └── decorators.ts              # Class and method decorators
└── javascript/
    ├── simple_function.js
    ├── class_with_methods.js
    ├── es_module_exports.js
    ├── arrow_functions.js          # Block-bodied and expression-bodied
    └── decorators.js               # Class and method decorators
```

### Test Categories

| Category | What It Validates |
|---|---|
| Per-language basics | Each fixture produces a correct skeleton — signatures preserved, bodies replaced, data nodes intact |
| Doc comment preservation | Doc comments kept, inline comments inside bodies stripped with the body |
| Line count accuracy | `[skltn: N lines hidden]` matches actual line count of pruned body |
| Depth limiting | With `max_depth: 1`, nested structural nodes emit verbatim |
| Error tolerance | Files with syntax errors produce partial skeletons + warning comments |
| Closure/lambda handling | Block-bodied closures pruned, expression-bodied and lambdas emitted verbatim |
| Container node recursion | impl blocks and classes correctly recurse without pruning their own body |
| Syntactic validity (round-trip) | Skeleton output parses via tree-sitter with zero ERROR nodes |
| CRLF handling | Single test converting a fixture to `\r\n` and asserting correct skeleton output |

### Round-Trip Validation

The critical quality gate. Every skeleton is re-parsed by tree-sitter to confirm syntactic validity:

```rust
#[test]
fn test_rust_skeleton_is_valid_syntax() {
    let source = include_str!("../../../fixtures/rust/struct_with_methods.rs");
    let backend = RustBackend;
    let skeleton = SkeletonEngine::skeletonize(source, &backend, &default_opts()).unwrap();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(backend.language()).unwrap();
    let tree = parser.parse(&skeleton, None).unwrap();
    assert!(!has_error_nodes(tree.root_node()));
}
```

---

## Success Criteria (Phase 1)

| Metric | Target |
|---|---|
| Compression | >75% token reduction on fixture files |
| Latency | <30ms for files under 5,000 lines. Files exceeding 5,000 lines are still processed without artificial limits, but performance is best-effort. |
| Accuracy | Zero tree-sitter ERROR nodes in skeleton output (round-trip test) |
| Coverage | All 4 languages passing all test categories |

---

## Out of Scope (Phase 1)

- MCP server integration (Phase 2)
- Token counting / observability (Phase 3)
- Web Dashboard (Phase 4)
- Solidity (`.sol`) — deferred from PRD Phase 1; will be added as a standalone `SolidityBackend` in a future phase
- Other languages beyond the supported four
- Files without standard extensions (Dockerfile, Makefile, etc.)
- CLI integration tests (added in later phases)
