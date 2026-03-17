use skltn_obs::pricing::{calculate_cost, get_rates};

#[test]
fn test_opus_rates() {
    let rates = get_rates("claude-opus-4-6-20260301");
    assert_eq!(rates.input_per_mtok, 15.00);
    assert_eq!(rates.output_per_mtok, 75.00);
    assert_eq!(rates.cache_write_per_mtok, 18.75);
    assert_eq!(rates.cache_read_per_mtok, 1.50);
}

#[test]
fn test_sonnet_rates() {
    let rates = get_rates("claude-sonnet-4-6-20260301");
    assert_eq!(rates.input_per_mtok, 3.00);
    assert_eq!(rates.output_per_mtok, 15.00);
    assert_eq!(rates.cache_write_per_mtok, 3.75);
    assert_eq!(rates.cache_read_per_mtok, 0.30);
}

#[test]
fn test_haiku_rates() {
    let rates = get_rates("claude-haiku-4-5-20251001");
    assert_eq!(rates.input_per_mtok, 0.80);
    assert_eq!(rates.output_per_mtok, 4.00);
    assert_eq!(rates.cache_write_per_mtok, 1.00);
    assert_eq!(rates.cache_read_per_mtok, 0.08);
}

#[test]
fn test_legacy_sonnet_35_rates() {
    let rates = get_rates("claude-3-5-sonnet-20241022");
    assert_eq!(rates.input_per_mtok, 3.00);
    assert_eq!(rates.output_per_mtok, 15.00);
}

#[test]
fn test_legacy_sonnet_37_rates() {
    let rates = get_rates("claude-3-7-sonnet-20250219");
    assert_eq!(rates.input_per_mtok, 3.00);
    assert_eq!(rates.output_per_mtok, 15.00);
}

#[test]
fn test_legacy_haiku_35_rates() {
    let rates = get_rates("claude-3-5-haiku-20241022");
    assert_eq!(rates.input_per_mtok, 0.80);
    assert_eq!(rates.output_per_mtok, 4.00);
}

#[test]
fn test_unknown_model_returns_zero() {
    let rates = get_rates("gpt-4o-unknown");
    assert_eq!(rates.input_per_mtok, 0.0);
    assert_eq!(rates.output_per_mtok, 0.0);
    assert_eq!(rates.cache_write_per_mtok, 0.0);
    assert_eq!(rates.cache_read_per_mtok, 0.0);
}

#[test]
fn test_calculate_cost_opus() {
    let rates = get_rates("claude-opus-4-6-20260301");
    let cost = calculate_cost(2500, 800, 0, 1200, &rates);
    // (2500 * 15.00 + 800 * 75.00 + 0 * 18.75 + 1200 * 1.50) / 1_000_000
    // = (37500 + 60000 + 0 + 1800) / 1_000_000
    // = 99300 / 1_000_000
    // = 0.0993
    let expected = 0.0993;
    assert!((cost - expected).abs() < 1e-10, "Expected {expected}, got {cost}");
}

#[test]
fn test_calculate_cost_zero_tokens() {
    let rates = get_rates("claude-sonnet-4-6-20260301");
    let cost = calculate_cost(0, 0, 0, 0, &rates);
    assert_eq!(cost, 0.0);
}
