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
