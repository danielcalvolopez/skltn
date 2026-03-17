#[derive(Debug, Clone, PartialEq)]
pub struct ModelRates {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_write_per_mtok: f64,
    pub cache_read_per_mtok: f64,
}

impl ModelRates {
    pub fn zero() -> Self {
        Self {
            input_per_mtok: 0.0,
            output_per_mtok: 0.0,
            cache_write_per_mtok: 0.0,
            cache_read_per_mtok: 0.0,
        }
    }
}

/// Rates as of 2026-03-16. Verify against https://docs.anthropic.com/en/docs/about-claude/models
/// before implementation — prices may have changed.
pub fn get_rates(model: &str) -> ModelRates {
    match model {
        m if m.contains("claude-opus-4") => ModelRates {
            input_per_mtok: 15.00,
            output_per_mtok: 75.00,
            cache_write_per_mtok: 18.75,
            cache_read_per_mtok: 1.50,
        },
        m if m.contains("claude-sonnet-4") => ModelRates {
            input_per_mtok: 3.00,
            output_per_mtok: 15.00,
            cache_write_per_mtok: 3.75,
            cache_read_per_mtok: 0.30,
        },
        m if m.contains("claude-haiku-4") => ModelRates {
            input_per_mtok: 0.80,
            output_per_mtok: 4.00,
            cache_write_per_mtok: 1.00,
            cache_read_per_mtok: 0.08,
        },
        m if m.contains("claude-3-7-sonnet") => ModelRates {
            input_per_mtok: 3.00,
            output_per_mtok: 15.00,
            cache_write_per_mtok: 3.75,
            cache_read_per_mtok: 0.30,
        },
        m if m.contains("claude-3-5-sonnet") => ModelRates {
            input_per_mtok: 3.00,
            output_per_mtok: 15.00,
            cache_write_per_mtok: 3.75,
            cache_read_per_mtok: 0.30,
        },
        m if m.contains("claude-3-5-haiku") => ModelRates {
            input_per_mtok: 0.80,
            output_per_mtok: 4.00,
            cache_write_per_mtok: 1.00,
            cache_read_per_mtok: 0.08,
        },
        _ => {
            tracing::warn!("Unknown model '{}', cost tracking will show $0.00", model);
            ModelRates::zero()
        }
    }
}

pub fn calculate_cost(
    input_tokens: usize,
    output_tokens: usize,
    cache_creation_input_tokens: usize,
    cache_read_input_tokens: usize,
    rates: &ModelRates,
) -> f64 {
    (input_tokens as f64 * rates.input_per_mtok
        + output_tokens as f64 * rates.output_per_mtok
        + cache_creation_input_tokens as f64 * rates.cache_write_per_mtok
        + cache_read_input_tokens as f64 * rates.cache_read_per_mtok)
        / 1_000_000.0
}
