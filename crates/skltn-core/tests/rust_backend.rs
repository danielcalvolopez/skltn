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
