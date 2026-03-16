mod common;

use common::{default_opts, has_error_nodes};
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
