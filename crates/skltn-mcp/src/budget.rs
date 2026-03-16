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
        BudgetDecision::Skeletonize {
            original_tokens: token_count,
        }
    } else {
        BudgetDecision::ReturnFull {
            original_tokens: token_count,
        }
    }
}

pub fn count_tokens(text: &str, tokenizer: &CoreBPE) -> usize {
    tokenizer.encode_ordinary(text).len()
}
