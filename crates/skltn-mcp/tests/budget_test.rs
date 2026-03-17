use tiktoken_rs::CoreBPE;

use skltn_mcp::budget::CacheHint;

fn tokenizer() -> CoreBPE {
    tiktoken_rs::cl100k_base().unwrap()
}

#[test]
fn test_small_file_returns_full() {
    let source = "fn main() {\n    println!(\"hello\");\n}\n";
    let tokenizer = tokenizer();
    let decision = skltn_mcp::budget::should_skeletonize(source, &tokenizer, CacheHint::Unknown);
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
    let mut source = String::new();
    for i in 0..500 {
        source.push_str(&format!(
            "fn function_{i}(arg: i32) -> i32 {{\n    arg + {i}\n}}\n\n"
        ));
    }
    let tokenizer = tokenizer();
    let decision = skltn_mcp::budget::should_skeletonize(&source, &tokenizer, CacheHint::Unknown);
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

#[test]
fn test_unknown_hint_small_file_returns_full() {
    let source = "fn main() {\n    println!(\"hello\");\n}\n";
    let tokenizer = tokenizer();
    let decision = skltn_mcp::budget::should_skeletonize(source, &tokenizer, CacheHint::Unknown);
    match decision {
        skltn_mcp::budget::BudgetDecision::ReturnFull { original_tokens } => {
            assert!(original_tokens <= 2000);
            assert!(original_tokens > 0);
        }
        _ => panic!("Expected ReturnFull for small file with Unknown hint"),
    }
}

#[test]
fn test_unknown_hint_large_file_returns_skeletonize() {
    let mut source = String::new();
    for i in 0..500 {
        source.push_str(&format!(
            "fn function_{i}(arg: i32) -> i32 {{\n    arg + {i}\n}}\n\n"
        ));
    }
    let tokenizer = tokenizer();
    let decision = skltn_mcp::budget::should_skeletonize(&source, &tokenizer, CacheHint::Unknown);
    match decision {
        skltn_mcp::budget::BudgetDecision::Skeletonize { original_tokens } => {
            assert!(original_tokens > 2000);
        }
        _ => panic!("Expected Skeletonize for large file with Unknown hint"),
    }
}

#[test]
fn test_recently_served_hint_large_file_returns_full() {
    let mut source = String::new();
    for i in 0..500 {
        source.push_str(&format!(
            "fn function_{i}(arg: i32) -> i32 {{\n    arg + {i}\n}}\n\n"
        ));
    }
    let tokenizer = tokenizer();
    let decision =
        skltn_mcp::budget::should_skeletonize(&source, &tokenizer, CacheHint::RecentlyServed);
    match decision {
        skltn_mcp::budget::BudgetDecision::ReturnFull { original_tokens } => {
            assert!(original_tokens > 2000);
        }
        _ => panic!("Expected ReturnFull for large file with RecentlyServed hint"),
    }
}

#[test]
fn test_cache_confirmed_hint_large_file_returns_full() {
    let mut source = String::new();
    for i in 0..500 {
        source.push_str(&format!(
            "fn function_{i}(arg: i32) -> i32 {{\n    arg + {i}\n}}\n\n"
        ));
    }
    let tokenizer = tokenizer();
    let decision =
        skltn_mcp::budget::should_skeletonize(&source, &tokenizer, CacheHint::CacheConfirmed);
    match decision {
        skltn_mcp::budget::BudgetDecision::ReturnFull { original_tokens } => {
            assert!(original_tokens > 2000);
        }
        _ => panic!("Expected ReturnFull for large file with CacheConfirmed hint"),
    }
}

#[test]
fn test_cache_expired_hint_large_file_returns_skeletonize() {
    let mut source = String::new();
    for i in 0..500 {
        source.push_str(&format!(
            "fn function_{i}(arg: i32) -> i32 {{\n    arg + {i}\n}}\n\n"
        ));
    }
    let tokenizer = tokenizer();
    let decision =
        skltn_mcp::budget::should_skeletonize(&source, &tokenizer, CacheHint::CacheExpired);
    match decision {
        skltn_mcp::budget::BudgetDecision::Skeletonize { original_tokens } => {
            assert!(original_tokens > 2000);
        }
        _ => panic!("Expected Skeletonize for large file with CacheExpired hint"),
    }
}
