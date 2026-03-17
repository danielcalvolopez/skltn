use tiktoken_rs::CoreBPE;

pub const TOKEN_THRESHOLD: usize = 2_000;

/// Hint about whether a file's content is likely in the provider's prompt cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheHint {
    /// No prior information — cold start, use token threshold heuristic.
    Unknown,
    /// File was served full recently in this session — likely cached by provider.
    RecentlyServed,
    /// Phase 3 integration: obs proxy confirmed cache_read_input_tokens > 0.
    CacheConfirmed,
    /// Phase 3 integration: obs data is stale (>5min since last cache hit).
    CacheExpired,
}

#[derive(Debug)]
pub enum BudgetDecision {
    Skeletonize { original_tokens: usize },
    ReturnFull { original_tokens: usize },
}

pub fn should_skeletonize(source: &str, tokenizer: &CoreBPE, hint: CacheHint) -> BudgetDecision {
    let token_count = tokenizer.encode_ordinary(source).len();

    match hint {
        // File is likely cached — serve full regardless of size
        CacheHint::RecentlyServed | CacheHint::CacheConfirmed => BudgetDecision::ReturnFull {
            original_tokens: token_count,
        },
        // No cache info or cache expired — fall back to token threshold
        CacheHint::Unknown | CacheHint::CacheExpired => {
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
    }
}

pub fn count_tokens(text: &str, tokenizer: &CoreBPE) -> usize {
    tokenizer.encode_ordinary(text).len()
}
