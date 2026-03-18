use axum::body::Body;
use axum::extract::{Request, State};
use axum::response::{IntoResponse, Response};
use http::header::{HOST, TRANSFER_ENCODING};

use crate::drilldown::DrilldownTracker;
use crate::savings::SavingsTracker;
use crate::skim::skim_nonstreaming;
use crate::tracker::CostTracker;

#[derive(Clone)]
pub struct AppState {
    pub client: reqwest::Client,
    pub upstream: String,
    pub tracker: CostTracker,
    pub savings_tracker: SavingsTracker,
    pub drilldown_tracker: DrilldownTracker,
}

/// Catch-all proxy handler. Forwards all requests to the upstream Anthropic API.
pub async fn proxy_handler(
    State(state): State<AppState>,
    req: Request,
) -> Result<Response, Response> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(|q| format!("?{q}")).unwrap_or_default();
    let upstream_url = format!("{}{path}{query}", state.upstream);

    // SECURITY: Log method and path only — never log headers (they contain x-api-key)
    tracing::debug!(%method, %path, "Proxying request");

    let headers = req.headers().clone();
    let body_bytes = axum::body::to_bytes(req.into_body(), 200 * 1024 * 1024)
        .await
        .map_err(|e| {
            tracing::error!("Failed to read request body: {e}");
            (http::StatusCode::BAD_REQUEST, "Failed to read request body").into_response()
        })?;

    let model = if method == http::Method::POST && path == "/v1/messages" {
        extract_model(&body_bytes)
    } else {
        None
    };

    let mut upstream_req = state.client.request(method, &upstream_url);
    for (key, value) in headers.iter() {
        if key == HOST || key == TRANSFER_ENCODING {
            continue;
        }
        upstream_req = upstream_req.header(key.as_str(), value);
    }
    upstream_req = upstream_req.body(body_bytes.to_vec());

    let upstream_resp = upstream_req.send().await.map_err(|e| {
        tracing::error!("Upstream request failed: {e}");
        (
            http::StatusCode::BAD_GATEWAY,
            format!("Upstream error: {e}"),
        )
            .into_response()
    })?;

    let status = upstream_resp.status();
    let resp_headers = upstream_resp.headers().clone();
    let content_type = resp_headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    tracing::debug!(%status, content_type, "Upstream response received");

    let is_streaming = content_type.contains("text/event-stream");

    if is_streaming {
        // Buffer the full streaming response — reqwest's bytes_stream() can be empty
        // when the body was already consumed during HTTP/2 framing.
        let resp_bytes = upstream_resp.bytes().await.map_err(|e| {
            tracing::error!("Failed to read streaming response body: {e}");
            (
                http::StatusCode::BAD_GATEWAY,
                "Failed to read upstream response",
            )
                .into_response()
        })?;

        let body_preview = String::from_utf8_lossy(&resp_bytes[..resp_bytes.len().min(500)]);
        tracing::debug!(body_len = resp_bytes.len(), body = %body_preview, "Buffered streaming response");

        let mut parts = http::Response::new(()).into_parts().0;
        parts.status = status;
        parts.headers = resp_headers;

        if let Some(ref model) = model {
            // Parse SSE events from the buffered body and extract usage
            crate::skim::skim_streaming_buffered(&resp_bytes, model, &state.tracker).await;
        }

        let mut response = Response::new(Body::from(resp_bytes));
        *response.status_mut() = parts.status;
        *response.headers_mut() = parts.headers;
        crate::skim::strip_hop_by_hop(response.headers_mut());
        Ok(response)
    } else {
        let resp_bytes = upstream_resp.bytes().await.map_err(|e| {
            tracing::error!("Failed to read upstream response body: {e}");
            (
                http::StatusCode::BAD_GATEWAY,
                "Failed to read upstream response",
            )
                .into_response()
        })?;

        let mut parts = http::Response::new(()).into_parts().0;
        parts.status = status;
        parts.headers = resp_headers;

        if let Some(ref model) = model {
            Ok(skim_nonstreaming(parts, resp_bytes, model, &state.tracker).await)
        } else {
            let mut response = Response::new(Body::from(resp_bytes));
            *response.status_mut() = parts.status;
            *response.headers_mut() = parts.headers;
            crate::skim::strip_hop_by_hop(response.headers_mut());
            Ok(response)
        }
    }
}

/// Extract the "model" field from a JSON request body.
/// Validates model name: alphanumeric, hyphens, dots, underscores only.
static MODEL_NAME_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^[a-zA-Z0-9._-]+$").unwrap());

fn extract_model(body: &[u8]) -> Option<String> {
    match serde_json::from_slice::<serde_json::Value>(body) {
        Ok(json) => {
            let raw = json.get("model").and_then(|v| v.as_str())?;
            if MODEL_NAME_RE.is_match(raw) {
                Some(raw.to_string())
            } else {
                tracing::warn!("Invalid model name '{raw}', replacing with 'unknown'");
                Some("unknown".to_string())
            }
        }
        Err(e) => {
            tracing::warn!("Failed to parse request body for model extraction: {e}");
            None
        }
    }
}
