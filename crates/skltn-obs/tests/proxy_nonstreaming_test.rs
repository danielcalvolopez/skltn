use axum::routing::{any, post};
use axum::Router;
use skltn_obs::proxy::{proxy_handler, AppState};
use skltn_obs::savings::SavingsTracker;
use skltn_obs::tracker::CostTracker;
use std::net::SocketAddr;
use tokio::net::TcpListener;

async fn mock_messages_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "id": "msg_test123",
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": "Hello!"}],
        "model": "claude-sonnet-4-6-20260301",
        "usage": {
            "input_tokens": 100,
            "output_tokens": 50,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0
        }
    }))
}

async fn mock_models_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "data": [{"id": "claude-sonnet-4-6", "type": "model"}]
    }))
}

async fn start_mock_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .route("/v1/messages", post(mock_messages_handler))
        .route("/v1/models", any(mock_models_handler));

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

    let savings_tracker = SavingsTracker::new(dir.path().to_path_buf()).await;
    let drilldown_tracker = skltn_obs::drilldown::DrilldownTracker::new(dir.path().to_path_buf()).await;
    let state = AppState {
        client,
        upstream: format!("http://{upstream_addr}"),
        tracker: tracker.clone(),
        savings_tracker,
        drilldown_tracker,
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
async fn test_nonstreaming_proxy_extracts_usage() {
    let (mock_addr, _mock_handle) = start_mock_server().await;
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
            "messages": [{"role": "user", "content": "Hi"}]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["id"], "msg_test123");
    assert_eq!(body["usage"]["input_tokens"], 100);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let records = tracker.records().await;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].model, "claude-sonnet-4-6-20260301");
    assert_eq!(records[0].input_tokens, 100);
    assert_eq!(records[0].output_tokens, 50);
    assert!(records[0].cost_usd > 0.0);
}

#[tokio::test]
async fn test_passthrough_non_message_endpoint() {
    let (mock_addr, _mock_handle) = start_mock_server().await;
    let (proxy_addr, tracker) = start_proxy(mock_addr).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{proxy_addr}/v1/models"))
        .header("x-api-key", "test-key")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("data").is_some());

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let records = tracker.records().await;
    assert_eq!(records.len(), 0);
}

#[tokio::test]
async fn test_proxy_returns_upstream_error_status() {
    let app = Router::new().fallback(|| async { (http::StatusCode::BAD_REQUEST, "bad request") });
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
        .json(&serde_json::json!({"model": "claude-sonnet-4-6", "bad": true}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let records = tracker.records().await;
    assert_eq!(
        records.len(),
        0,
        "Error responses should not create usage records"
    );
}
