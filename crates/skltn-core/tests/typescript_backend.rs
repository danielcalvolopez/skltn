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
