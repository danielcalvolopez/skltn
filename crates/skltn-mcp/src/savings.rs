use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use time::OffsetDateTime;

#[derive(Serialize, Clone, Debug)]
pub struct SavingsRecord {
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub file: String,
    pub language: String,
    pub original_tokens: usize,
    pub skeleton_tokens: usize,
    pub saved_tokens: usize,
}

/// Append-only writer for skeletonization savings records.
/// Writes one JSON line per skeletonization event to `~/.skltn/savings.jsonl`.
pub struct SavingsWriter {
    path: PathBuf,
}

impl SavingsWriter {
    pub fn new() -> Option<Self> {
        let dir = dirs::home_dir()?.join(".skltn");
        if let Err(e) = fs::create_dir_all(&dir) {
            tracing::error!("Failed to create savings directory: {e}");
            return None;
        }

        let path = match dir.canonicalize() {
            Ok(d) => d.join("savings.jsonl"),
            Err(e) => {
                tracing::error!("Failed to canonicalize savings directory: {e}");
                return None;
            }
        };

        Some(Self { path })
    }

    pub fn record(&self, record: SavingsRecord) {
        let json = match serde_json::to_string(&record) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("Failed to serialize SavingsRecord: {e}");
                return;
            }
        };

        let mut file = match OpenOptions::new().append(true).create(true).open(&self.path) {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("Failed to open savings.jsonl: {e}");
                return;
            }
        };

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
        }

        if let Err(e) = writeln!(file, "{json}") {
            tracing::error!("Failed to write savings record: {e}");
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct DrilldownRecord {
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub file: String,
    pub symbol: String,
    pub tokens: usize,
}

/// Append-only writer for symbol drilldown records.
/// Writes one JSON line per drilldown event to `~/.skltn/drilldowns.jsonl`.
pub struct DrilldownWriter {
    path: PathBuf,
}

impl DrilldownWriter {
    pub fn new() -> Option<Self> {
        let dir = dirs::home_dir()?.join(".skltn");
        if let Err(e) = fs::create_dir_all(&dir) {
            tracing::error!("Failed to create drilldowns directory: {e}");
            return None;
        }

        let path = match dir.canonicalize() {
            Ok(d) => d.join("drilldowns.jsonl"),
            Err(e) => {
                tracing::error!("Failed to canonicalize drilldowns directory: {e}");
                return None;
            }
        };

        Some(Self { path })
    }

    pub fn record(&self, record: DrilldownRecord) {
        let json = match serde_json::to_string(&record) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("Failed to serialize DrilldownRecord: {e}");
                return;
            }
        };

        let mut file = match OpenOptions::new().append(true).create(true).open(&self.path) {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("Failed to open drilldowns.jsonl: {e}");
                return;
            }
        };

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
        }

        if let Err(e) = writeln!(file, "{json}") {
            tracing::error!("Failed to write drilldown record: {e}");
        }
    }
}
