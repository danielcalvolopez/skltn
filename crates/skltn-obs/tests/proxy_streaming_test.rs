use axum::body::Body;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use futures::stream;
use skltn_obs::proxy::{proxy_handler, AppState};
use skltn_obs::tracker::CostTracker;
use std::net::SocketAddr;
use tokio::net::TcpListener;

fn sse_body_from_strings(chunks: Vec<String>) -> Body {
    let byte_stream = stream::iter(
        chunks
            .into_iter()
            .map(|s| Ok::<bytes::Bytes, std::io::Error>(bytes::Bytes::from(s))),
    );
    Body::from_stream(byte_stream)
}

fn mock_sse_body() -> Body {
    sse_body_from_strings(vec![
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_test\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-4-6-20260301\",\"usage\":{\"input_tokens\":150,\"cache_creation_input_tokens\":0,\"cache_read_input_tokens\":50}}}\n\n".to_string(),
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n".to_string(),
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello!\"}}\n\n".to_string(),
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n".to_string(),
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":75}}\n\n".to_string(),
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_string(),
    ])
}

async fn mock_streaming_handler() -> Response {
    let body = mock_sse_body();
    ([("content-type", "text/event-stream")], body).into_response()
}

async fn start_mock_sse_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let app = Router::new().route("/v1/messages", post(mock_streaming_handler));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, handle)
}

async fn start_proxy(upstream_addr: SocketAddr) -> (SocketAddr, CostTracker) {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .tcp_nodelay(true)
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .unwrap();

    let state = AppState {
        client,
        upstream: format!("http://{upstream_addr}"),
        tracker: tracker.clone(),
    };

    let app = Router::new().fallback(proxy_handler).with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, tracker)
}

#[tokio::test]
async fn test_streaming_proxy_extracts_usage() {
    let (mock_addr, _handle) = start_mock_sse_server().await;
    let (proxy_addr, tracker) = start_proxy(mock_addr).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{proxy_addr}/v1/messages"))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": "claude-sonnet-4-6-20260301",
            "max_tokens": 100,
            "stream": true,
            "messages": [{"role": "user", "content": "Hi"}]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body_text = resp.text().await.unwrap();
    assert!(body_text.contains("message_start"));
    assert!(body_text.contains("message_delta"));
    assert!(body_text.contains("message_stop"));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let records = tracker.records().await;
    assert_eq!(records.len(), 1, "Expected 1 usage record, got {}", records.len());
    assert_eq!(records[0].model, "claude-sonnet-4-6-20260301");
    assert_eq!(records[0].input_tokens, 150);
    assert_eq!(records[0].output_tokens, 75);
    assert_eq!(records[0].cache_read_input_tokens, 50);
    assert!(records[0].cost_usd > 0.0);
}

async fn mock_split_chunks_handler() -> Response {
    let body = sse_body_from_strings(vec![
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":200,\"cache_creation_".to_string(),
        "input_tokens\":0,\"cache_read_input_tokens\":0}}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n".to_string(),
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":30}}\n\n".to_string(),
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_string(),
    ]);
    ([("content-type", "text/event-stream")], body).into_response()
}

async fn mock_incomplete_stream_handler() -> Response {
    let body = sse_body_from_strings(vec![
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":100,\"cache_creation_input_tokens\":0,\"cache_read_input_tokens\":0}}}\n\n".to_string(),
    ]);
    ([("content-type", "text/event-stream")], body).into_response()
}

#[tokio::test]
async fn test_streaming_split_chunks() {
    let app = Router::new().route("/v1/messages", post(mock_split_chunks_handler));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mock_addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let (proxy_addr, tracker) = start_proxy(mock_addr).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{proxy_addr}/v1/messages"))
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": "claude-sonnet-4-6-20260301",
            "max_tokens": 100,
            "stream": true,
            "messages": [{"role": "user", "content": "Hi"}]
        }))
        .send()
        .await
        .unwrap();

    let _ = resp.text().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let records = tracker.records().await;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].input_tokens, 200);
    assert_eq!(records[0].output_tokens, 30);
}

#[tokio::test]
async fn test_streaming_incomplete_discards_record() {
    let app = Router::new().route("/v1/messages", post(mock_incomplete_stream_handler));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mock_addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let (proxy_addr, tracker) = start_proxy(mock_addr).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{proxy_addr}/v1/messages"))
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": "claude-sonnet-4-6-20260301",
            "max_tokens": 100,
            "stream": true,
            "messages": [{"role": "user", "content": "Hi"}]
        }))
        .send()
        .await
        .unwrap();

    let _ = resp.text().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let records = tracker.records().await;
    assert_eq!(
        records.len(),
        0,
        "Expected no records for incomplete stream, got {}",
        records.len()
    );
}
