use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::io::AsyncWriteExt;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::task::JoinHandle;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UsageRecord {
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub model: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cache_creation_input_tokens: usize,
    pub cache_read_input_tokens: usize,
    pub cost_usd: f64,
}

#[derive(Clone)]
pub struct CostTracker {
    inner: Arc<Mutex<CostTrackerInner>>,
    writer_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

struct CostTrackerInner {
    records: Vec<UsageRecord>,
    file_writer: mpsc::Sender<UsageRecord>,
    broadcast: broadcast::Sender<UsageRecord>,
}

impl CostTracker {
    pub async fn new(data_dir: PathBuf) -> Self {
        tokio::fs::create_dir_all(&data_dir)
            .await
            .expect("failed to create data directory");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o700);
            tokio::fs::set_permissions(&data_dir, perms).await.ok();
        }

        let (file_tx, file_rx) = mpsc::channel::<UsageRecord>(256);
        let (broadcast_tx, _) = broadcast::channel::<UsageRecord>(64);

        let handle = spawn_jsonl_writer(data_dir.join("usage.jsonl"), file_rx);

        Self {
            inner: Arc::new(Mutex::new(CostTrackerInner {
                records: Vec::new(),
                file_writer: file_tx,
                broadcast: broadcast_tx,
            })),
            writer_handle: Arc::new(Mutex::new(Some(handle))),
        }
    }

    pub async fn record(&self, record: UsageRecord) {
        let inner = &mut *self.inner.lock().await;
        inner.records.push(record.clone());
        let _ = inner.broadcast.send(record.clone());
        if let Err(e) = inner.file_writer.send(record).await {
            tracing::error!("JSONL writer channel closed, record lost: {e}");
        }
    }

    pub async fn records(&self) -> Vec<UsageRecord> {
        self.inner.lock().await.records.clone()
    }

    pub async fn subscribe(&self) -> broadcast::Receiver<UsageRecord> {
        self.inner.lock().await.broadcast.subscribe()
    }

    /// Returns (records snapshot, broadcast receiver) under the same lock.
    /// This is the critical invariant for gap-free WebSocket replay-to-live handoff.
    pub async fn snapshot_and_subscribe(
        &self,
    ) -> (Vec<UsageRecord>, broadcast::Receiver<UsageRecord>) {
        let inner = self.inner.lock().await;
        (inner.records.clone(), inner.broadcast.subscribe())
    }

    /// Gracefully shuts down the JSONL writer: drops the mpsc sender so the writer
    /// task drains remaining records, then awaits the task to completion.
    pub async fn shutdown(self) {
        drop(self.inner);
        if let Some(handle) = self.writer_handle.lock().await.take() {
            let _ = handle.await;
        }
    }
}

fn spawn_jsonl_writer(path: PathBuf, mut rx: mpsc::Receiver<UsageRecord>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut opts = tokio::fs::OpenOptions::new();
        opts.append(true).create(true);
        #[cfg(unix)]
        opts.mode(0o600);
        let mut file = opts.open(&path).await.expect("failed to open JSONL file");

        while let Some(record) = rx.recv().await {
            match serde_json::to_string(&record) {
                Ok(json) => {
                    let line = format!("{json}\n");
                    if let Err(e) = file.write_all(line.as_bytes()).await {
                        tracing::error!("Failed to write JSONL record: {e}");
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to serialize UsageRecord: {e}");
                }
            }
        }
        if let Err(e) = file.flush().await {
            tracing::error!("Failed to flush JSONL file: {e}");
        }
    })
}
