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
