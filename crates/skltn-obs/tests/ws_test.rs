use axum::routing::get;
use axum::Router;
use futures::StreamExt;
use skltn_obs::proxy::AppState;
use skltn_obs::tracker::{CostTracker, UsageRecord};
use skltn_obs::ws::ws_handler;
use std::net::SocketAddr;
use time::OffsetDateTime;
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;

fn sample_record(model: &str, input: usize, output: usize) -> UsageRecord {
    UsageRecord {
        timestamp: OffsetDateTime::now_utc(),
        model: model.to_string(),
        input_tokens: input,
        output_tokens: output,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
        cost_usd: 0.01,
    }
}

fn make_app_state(tracker: CostTracker) -> AppState {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    AppState {
        client,
        upstream: "http://unused".to_string(),
        tracker,
    }
}

async fn start_ws_server(tracker: CostTracker) -> SocketAddr {
    let state = make_app_state(tracker);
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test]
async fn test_ws_replay_existing_records() {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;

    tracker.record(sample_record("model-a", 100, 50)).await;
    tracker.record(sample_record("model-b", 200, 100)).await;

    let addr = start_ws_server(tracker).await;
    let (mut ws, _) = connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();

    let msg1 = ws.next().await.unwrap().unwrap();
    let record1: serde_json::Value = serde_json::from_str(&msg1.into_text().unwrap()).unwrap();
    assert_eq!(record1["model"], "model-a");

    let msg2 = ws.next().await.unwrap().unwrap();
    let record2: serde_json::Value = serde_json::from_str(&msg2.into_text().unwrap()).unwrap();
    assert_eq!(record2["model"], "model-b");
}

#[tokio::test]
async fn test_ws_live_records() {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;

    let addr = start_ws_server(tracker.clone()).await;
    let (mut ws, _) = connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    tracker.record(sample_record("model-live", 300, 150)).await;

    let msg = ws.next().await.unwrap().unwrap();
    let record: serde_json::Value = serde_json::from_str(&msg.into_text().unwrap()).unwrap();
    assert_eq!(record["model"], "model-live");
    assert_eq!(record["input_tokens"], 300);
}

#[tokio::test]
async fn test_ws_multiple_clients() {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;

    let addr = start_ws_server(tracker.clone()).await;

    let (mut ws1, _) = connect_async(format!("ws://{addr}/ws")).await.unwrap();
    let (mut ws2, _) = connect_async(format!("ws://{addr}/ws")).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    tracker
        .record(sample_record("model-broadcast", 400, 200))
        .await;

    let msg1 = ws1.next().await.unwrap().unwrap();
    let r1: serde_json::Value = serde_json::from_str(&msg1.into_text().unwrap()).unwrap();
    assert_eq!(r1["model"], "model-broadcast");

    let msg2 = ws2.next().await.unwrap().unwrap();
    let r2: serde_json::Value = serde_json::from_str(&msg2.into_text().unwrap()).unwrap();
    assert_eq!(r2["model"], "model-broadcast");
}

#[tokio::test]
async fn test_ws_replay_then_live() {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;

    tracker.record(sample_record("model-before", 100, 50)).await;

    let addr = start_ws_server(tracker.clone()).await;
    let (mut ws, _) = connect_async(format!("ws://{addr}/ws")).await.unwrap();

    let msg1 = ws.next().await.unwrap().unwrap();
    let r1: serde_json::Value = serde_json::from_str(&msg1.into_text().unwrap()).unwrap();
    assert_eq!(r1["model"], "model-before");

    tracker.record(sample_record("model-after", 200, 100)).await;

    let msg2 = ws.next().await.unwrap().unwrap();
    let r2: serde_json::Value = serde_json::from_str(&msg2.into_text().unwrap()).unwrap();
    assert_eq!(r2["model"], "model-after");
}
