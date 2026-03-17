use skltn_obs::tracker::{CostTracker, UsageRecord};
use time::OffsetDateTime;

fn sample_record(model: &str) -> UsageRecord {
    UsageRecord {
        timestamp: OffsetDateTime::now_utc(),
        model: model.to_string(),
        input_tokens: 1000,
        output_tokens: 500,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 200,
        cost_usd: 0.05,
    }
}

#[tokio::test]
async fn test_record_stores_in_memory() {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;
    let record = sample_record("claude-sonnet-4-6");

    tracker.record(record).await;

    let records = tracker.records().await;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].model, "claude-sonnet-4-6");
}

#[tokio::test]
async fn test_multiple_records() {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;

    tracker.record(sample_record("claude-sonnet-4-6")).await;
    tracker.record(sample_record("claude-opus-4-6")).await;
    tracker.record(sample_record("claude-haiku-4-5")).await;

    let records = tracker.records().await;
    assert_eq!(records.len(), 3);
}

#[tokio::test]
async fn test_broadcast_receives_records() {
    let dir = tempfile::tempdir().unwrap();
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;
    let mut rx = tracker.subscribe().await;

    let record = sample_record("claude-opus-4-6");
    tracker.record(record).await;

    let received = rx.recv().await.unwrap();
    assert_eq!(received.model, "claude-opus-4-6");
}

#[tokio::test]
async fn test_jsonl_file_written() {
    let dir = tempfile::tempdir().unwrap();
    let jsonl_path = dir.path().join("usage.jsonl");
    let tracker = CostTracker::new(dir.path().to_path_buf()).await;

    tracker.record(sample_record("claude-sonnet-4-6")).await;
    tracker.record(sample_record("claude-opus-4-6")).await;

    tracker.shutdown().await;

    let contents = std::fs::read_to_string(&jsonl_path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 2, "Expected 2 JSONL lines, got {}", lines.len());

    let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert!(parsed.get("model").is_some());
    assert!(parsed.get("cost_usd").is_some());
    assert!(parsed.get("timestamp").is_some());
}

#[tokio::test]
async fn test_jsonl_file_created_in_data_dir() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("subdir");
    let tracker = CostTracker::new(data_dir.clone()).await;

    tracker.record(sample_record("claude-sonnet-4-6")).await;
    tracker.shutdown().await;

    assert!(data_dir.join("usage.jsonl").exists());
}
