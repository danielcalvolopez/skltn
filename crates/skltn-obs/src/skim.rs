use crate::pricing::{calculate_cost, get_rates};
use crate::tracker::{CostTracker, UsageRecord};
use axum::body::Body;
use axum::response::Response;
use futures::stream::StreamExt;
use http::response::Parts;
use time::OffsetDateTime;
use tokio::sync::mpsc as tokio_mpsc;

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
    // Strip hop-by-hop headers — reqwest already decoded the transfer encoding,
    // so forwarding these causes hyper to double-encode or misframe the response.
    strip_hop_by_hop(response.headers_mut());
    response
}

/// Remove hop-by-hop headers that must not be forwarded by a proxy.
/// These are connection-specific and lose meaning when the body has been re-buffered.
pub(crate) fn strip_hop_by_hop(headers: &mut http::HeaderMap) {
    headers.remove(http::header::TRANSFER_ENCODING);
    headers.remove(http::header::CONNECTION);
    headers.remove(http::header::CONTENT_LENGTH);
}

/// Parses SSE events from a fully-buffered streaming response body and records usage.
/// Used when reqwest has already consumed the body (e.g. HTTP/2 responses where
/// bytes_stream() returns empty).
pub async fn skim_streaming_buffered(
    body: &[u8],
    model: &str,
    tracker: &CostTracker,
) {
    let text = match std::str::from_utf8(body) {
        Ok(t) => t,
        Err(_) => return,
    };

    let mut input_tokens: Option<usize> = None;
    let mut output_tokens: Option<usize> = None;
    let mut cache_creation_input_tokens: usize = 0;
    let mut cache_read_input_tokens: usize = 0;
    let mut saw_message_start = false;

    // Split on double-newline (handles both \n\n and \r\n\r\n)
    for event_block in text.split("\n\n") {
        let event_block = event_block.trim();
        if event_block.is_empty() {
            continue;
        }

        let mut event_type = None;
        let mut event_data = None;
        for line in event_block.lines() {
            let line = line.trim_start_matches('\r');
            if let Some(t) = line.strip_prefix("event: ") {
                event_type = Some(t);
            } else if let Some(d) = line.strip_prefix("data: ") {
                event_data = Some(d);
            }
        }

        if let (Some(etype), Some(data)) = (event_type, event_data) {
            match etype {
                "message_start" => {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                        if let Some(usage) = json.pointer("/message/usage") {
                            input_tokens = usage
                                .get("input_tokens")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as usize);
                            cache_creation_input_tokens = usage
                                .get("cache_creation_input_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0)
                                as usize;
                            cache_read_input_tokens = usage
                                .get("cache_read_input_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0)
                                as usize;
                            saw_message_start = true;
                        }
                    }
                }
                "message_delta" => {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                        if let Some(usage) = json.get("usage") {
                            output_tokens = usage
                                .get("output_tokens")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as usize);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if saw_message_start {
        if let (Some(input), Some(output)) = (input_tokens, output_tokens) {
            let rates = get_rates(model);
            let cost = calculate_cost(
                input,
                output,
                cache_creation_input_tokens,
                cache_read_input_tokens,
                &rates,
            );
            let record = UsageRecord {
                timestamp: OffsetDateTime::now_utc(),
                model: model.to_string(),
                input_tokens: input,
                output_tokens: output,
                cache_creation_input_tokens,
                cache_read_input_tokens,
                cost_usd: cost,
            };
            tracing::info!(model, input, output, cost, "Streaming usage recorded");
            tracker.record(record).await;
        }
    } else {
        tracing::warn!("No message_start found in buffered SSE response");
    }
}

/// Processes a streaming SSE response using the "background tee" approach.
/// Chunks are forwarded immediately to the client via a channel.
/// A background task clones each chunk, parses SSE events, and extracts usage data
/// from `message_start` and `message_delta` events.
pub fn skim_streaming(
    parts: Parts,
    upstream_stream: impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
    model: String,
    tracker: CostTracker,
) -> Response {
    let (tx, rx) = tokio_mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(64);

    tokio::spawn(async move {
        let mut stream = std::pin::pin!(upstream_stream);
        let mut buffer = String::new();
        const MAX_SSE_BUFFER: usize = 10 * 1024 * 1024; // 10 MB
        let mut input_tokens: Option<usize> = None;
        let mut output_tokens: Option<usize> = None;
        let mut cache_creation_input_tokens: usize = 0;
        let mut cache_read_input_tokens: usize = 0;
        let mut saw_message_start = false;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    if tx.send(Ok(chunk.clone())).await.is_err() {
                        return; // Client disconnected
                    }

                    if let Ok(text) = std::str::from_utf8(&chunk) {
                        tracing::debug!(chunk_len = chunk.len(), chunk_text = %text.chars().take(200).collect::<String>(), "SSE chunk received");
                        buffer.push_str(text);
                    }

                    if buffer.len() > MAX_SSE_BUFFER {
                        tracing::warn!(
                            "SSE buffer exceeded 10 MB without event boundary, discarding"
                        );
                        buffer.clear();
                        while let Some(chunk_result) = stream.next().await {
                            if let Ok(chunk) = chunk_result {
                                let _ = tx.send(Ok(chunk)).await;
                            }
                        }
                        return;
                    }

                    while let Some(boundary) = buffer.find("\n\n") {
                        let event_block = buffer[..boundary].to_string();
                        buffer = buffer[boundary + 2..].to_string();

                        let mut event_type = None;
                        let mut event_data = None;
                        for line in event_block.lines() {
                            if let Some(t) = line.strip_prefix("event: ") {
                                event_type = Some(t.to_string());
                            } else if let Some(d) = line.strip_prefix("data: ") {
                                event_data = Some(d.to_string());
                            }
                        }

                        if let Some(ref etype) = event_type {
                            tracing::debug!(event_type = %etype, "SSE event parsed");
                        }

                        if let (Some(ref etype), Some(ref data)) = (&event_type, &event_data) {
                            match etype.as_str() {
                                "message_start" => {
                                    if let Ok(json) =
                                        serde_json::from_str::<serde_json::Value>(data)
                                    {
                                        if let Some(usage) = json.pointer("/message/usage") {
                                            input_tokens = usage
                                                .get("input_tokens")
                                                .and_then(|v| v.as_u64())
                                                .map(|v| v as usize);
                                            cache_creation_input_tokens = usage
                                                .get("cache_creation_input_tokens")
                                                .and_then(|v| v.as_u64())
                                                .unwrap_or(0)
                                                as usize;
                                            cache_read_input_tokens = usage
                                                .get("cache_read_input_tokens")
                                                .and_then(|v| v.as_u64())
                                                .unwrap_or(0)
                                                as usize;
                                            saw_message_start = true;
                                        }
                                    }
                                }
                                "message_delta" => {
                                    if let Ok(json) =
                                        serde_json::from_str::<serde_json::Value>(data)
                                    {
                                        if let Some(usage) = json.get("usage") {
                                            output_tokens = usage
                                                .get("output_tokens")
                                                .and_then(|v| v.as_u64())
                                                .map(|v| v as usize);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Upstream stream error: {e}");
                    break;
                }
            }
        }

        // Stream ended — emit UsageRecord if we have complete data
        tracing::debug!(saw_message_start, ?input_tokens, ?output_tokens, "Stream ended");
        if saw_message_start {
            if let (Some(input), Some(output)) = (input_tokens, output_tokens) {
                let rates = get_rates(&model);
                let cost = calculate_cost(
                    input,
                    output,
                    cache_creation_input_tokens,
                    cache_read_input_tokens,
                    &rates,
                );
                let record = UsageRecord {
                    timestamp: OffsetDateTime::now_utc(),
                    model,
                    input_tokens: input,
                    output_tokens: output,
                    cache_creation_input_tokens,
                    cache_read_input_tokens,
                    cost_usd: cost,
                };
                tracker.record(record).await;
            } else {
                tracing::warn!(
                    "Incomplete streaming response: message_start received but missing {}",
                    if input_tokens.is_none() {
                        "input_tokens"
                    } else {
                        "output_tokens (no message_delta)"
                    }
                );
            }
        }
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    let body = Body::from_stream(stream);
    let mut response = Response::new(body);
    *response.status_mut() = parts.status;
    *response.headers_mut() = parts.headers;
    strip_hop_by_hop(response.headers_mut());
    response
}
