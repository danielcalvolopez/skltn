use crate::pricing::{calculate_cost, get_rates};
use crate::tracker::{CostTracker, UsageRecord};
use axum::body::Body;
use axum::response::Response;
use http::response::Parts;
use time::OffsetDateTime;

/// Extracts usage from a buffered non-streaming JSON response.
/// Returns the original bytes unchanged as an axum Response, and records usage if found.
pub async fn skim_nonstreaming(
    parts: Parts,
    body_bytes: bytes::Bytes,
    model: &str,
    tracker: &CostTracker,
) -> Response {
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
        if let Some(usage) = json.get("usage") {
            let input_tokens = usage
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let output_tokens = usage
                .get("output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let cache_creation_input_tokens = usage
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let cache_read_input_tokens = usage
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;

            let rates = get_rates(model);
            let cost = calculate_cost(
                input_tokens,
                output_tokens,
                cache_creation_input_tokens,
                cache_read_input_tokens,
                &rates,
            );

            let record = UsageRecord {
                timestamp: OffsetDateTime::now_utc(),
                model: model.to_string(),
                input_tokens,
                output_tokens,
                cache_creation_input_tokens,
                cache_read_input_tokens,
                cost_usd: cost,
            };

            tracker.record(record).await;
        }
    }

    let mut response = Response::new(Body::from(body_bytes));
    *response.status_mut() = parts.status;
    *response.headers_mut() = parts.headers;
    *response.version_mut() = parts.version;
    response
}
