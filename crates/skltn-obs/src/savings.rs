use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::{broadcast, Mutex};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavingsRecord {
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub file: String,
    pub language: String,
    pub original_tokens: usize,
    pub skeleton_tokens: usize,
    pub saved_tokens: usize,
}

#[derive(Clone)]
pub struct SavingsTracker {
    inner: Arc<Mutex<SavingsTrackerInner>>,
}

struct SavingsTrackerInner {
    records: Vec<SavingsRecord>,
    broadcast: broadcast::Sender<SavingsRecord>,
}

impl SavingsTracker {
    /// Create a new SavingsTracker that watches `savings.jsonl` in the given data directory.
    /// Truncates the file on startup (session-scoped data) and starts watching for new records.
    pub async fn new(data_dir: PathBuf) -> Self {
        let path = data_dir.join("savings.jsonl");

        // Truncate or create the file — start fresh each session
        if let Err(e) = tokio::fs::write(&path, b"").await {
            tracing::warn!("Failed to truncate savings.jsonl: {e}");
        }

        let (broadcast_tx, _) = broadcast::channel::<SavingsRecord>(64);

        let tracker = Self {
            inner: Arc::new(Mutex::new(SavingsTrackerInner {
                records: Vec::new(),
                broadcast: broadcast_tx,
            })),
        };

        // Spawn file watcher
        let watcher_tracker = tracker.clone();
        let watcher_path = path.clone();
        tokio::spawn(async move {
            if let Err(e) = watch_savings_file(watcher_path, watcher_tracker).await {
                tracing::error!("Savings file watcher failed: {e}");
            }
        });

        tracker
    }

    async fn push(&self, record: SavingsRecord) {
        let inner = &mut *self.inner.lock().await;
        inner.records.push(record.clone());
        let _ = inner.broadcast.send(record);
    }

    /// Returns (records snapshot, broadcast receiver) under the same lock.
    pub async fn snapshot_and_subscribe(
        &self,
    ) -> (Vec<SavingsRecord>, broadcast::Receiver<SavingsRecord>) {
        let inner = self.inner.lock().await;
        (inner.records.clone(), inner.broadcast.subscribe())
    }
}

async fn watch_savings_file(path: PathBuf, tracker: SavingsTracker) -> Result<(), String> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(16);

    let watch_path = path.clone();
    let _watcher = {
        let tx = tx.clone();
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                        let _ = tx.blocking_send(());
                    }
                }
            },
            notify::Config::default(),
        )
        .map_err(|e| format!("Failed to create file watcher: {e}"))?;

        // Watch the parent directory (the file may not exist yet)
        let parent = watch_path.parent().ok_or("No parent directory")?;
        watcher
            .watch(parent, RecursiveMode::NonRecursive)
            .map_err(|e| format!("Failed to watch directory: {e}"))?;

        watcher
    };

    let mut offset: u64 = 0;

    // Process file change notifications
    while rx.recv().await.is_some() {
        // Drain any queued notifications
        while rx.try_recv().is_ok() {}

        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let metadata = match file.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        // File was truncated (new session) — reset offset
        if metadata.len() < offset {
            offset = 0;
            // Also clear in-memory records since file was reset
        }

        if metadata.len() == offset {
            continue;
        }

        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(offset)).is_err() {
            continue;
        }

        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(n) => {
                    offset += n as u64;
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<SavingsRecord>(trimmed) {
                        Ok(record) => tracker.push(record).await,
                        Err(e) => {
                            tracing::warn!("Failed to parse savings record: {e}");
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Error reading savings.jsonl: {e}");
                    break;
                }
            }
        }
    }

    Ok(())
}
