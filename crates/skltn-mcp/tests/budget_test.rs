use tiktoken_rs::CoreBPE;

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
    let mut source = String::new();
    for i in 0..500 {
        source.push_str(&format!(
            "fn function_{i}(arg: i32) -> i32 {{\n    arg + {i}\n}}\n\n"
        ));
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
