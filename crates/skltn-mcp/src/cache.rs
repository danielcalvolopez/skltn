use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

/// Duration between automatic manifest flushes.
const AUTO_FLUSH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// Cached skeleton output for a single file.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CacheEntry {
    pub content_hash: String,
    pub mtime_secs: i64,
    pub original_tokens: usize,
    pub skeleton_tokens: usize,
    pub has_parse_errors: bool,
    pub skeleton: String,
}

/// Manifest of files read during a session, persisted as JSON.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionManifest {
    pub version: u32,
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub files: Vec<String>,
}

/// Per-project, file-based skeleton cache.
///
/// Stores one JSON file per cached skeleton under
/// `~/.skltn/cache/<project-hash>/`. Invalidation uses a two-tier strategy:
/// mtime as a cheap pre-filter, content hash as the authoritative check.
///
/// Also tracks which files were read during the current session via a
/// [`SessionManifest`], flushed periodically and on drop.
pub struct SkeletonCache {
    cache_dir: PathBuf,
    project_root: PathBuf,
    manifest_entries: Mutex<Vec<String>>,
    last_flush: Mutex<Instant>,
    manifest_dirty: Mutex<bool>,
    has_rotated: Mutex<bool>,
}

impl SkeletonCache {
    /// Create a new cache for the given project root.
    /// Creates the cache directory if needed and runs startup cleanup.
    pub fn new(project_root: &Path) -> Option<Self> {
        let home = dirs::home_dir()?;
        let skltn_dir = home.join(".skltn").join("cache");

        let project_hash = {
            let mut hasher = Sha256::new();
            hasher.update(project_root.to_string_lossy().as_bytes());
            let result = hasher.finalize();
            hex::encode(&result[..8]) // 16 hex chars
        };

        let cache_dir = skltn_dir.join(project_hash);

        if let Err(e) = fs::create_dir_all(&cache_dir) {
            tracing::error!("Failed to create skeleton cache directory: {e}");
            return None;
        }

        let cache_dir = match cache_dir.canonicalize() {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("Failed to canonicalize cache directory: {e}");
                return None;
            }
        };

        let cache = Self {
            cache_dir,
            project_root: project_root.to_path_buf(),
            manifest_entries: Mutex::new(Vec::new()),
            last_flush: Mutex::new(Instant::now()),
            manifest_dirty: Mutex::new(false),
            has_rotated: Mutex::new(false),
        };

        cache.cleanup();

        Some(cache)
    }

    /// Two-tier cache lookup: mtime first, then content hash.
    /// Updates stored mtime on content-hash hit to avoid future hashing.
    pub fn get_with_validation(
        &self,
        file: &str,
        current_mtime: i64,
        source: &str,
    ) -> Option<CacheEntry> {
        let entry = self.load(file)?;

        // Fast path: mtime unchanged
        if entry.mtime_secs == current_mtime {
            return Some(entry);
        }

        // Slow path: mtime changed, check content hash
        let current_hash = hash_content(source);
        if entry.content_hash == current_hash {
            // Content identical — update mtime to avoid future hashing
            let mut updated = entry;
            updated.mtime_secs = current_mtime;
            self.store(file, &updated);
            return Some(updated);
        }

        None // Real cache miss — content changed
    }

    /// Store a skeleton cache entry for a file.
    pub fn store(&self, file: &str, entry: &CacheEntry) {
        let path = self.entry_path(file);

        let json = match serde_json::to_string_pretty(entry) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("Failed to serialize cache entry for {file}: {e}");
                return;
            }
        };

        if let Err(e) = fs::write(&path, json.as_bytes()) {
            tracing::error!("Failed to write cache entry for {file}: {e}");
            return;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
    }

    // ── Manifest tracking ────────────────────────────────────────────

    /// Record that a file was read in this session. Deduplicates while
    /// preserving insertion order. Auto-flushes every 5 seconds.
    pub fn record_manifest_entry(&self, file: &str) {
        let mut entries = self.manifest_entries.lock().unwrap();
        if !entries.iter().any(|e| e == file) {
            entries.push(file.to_owned());
        }
        *self.manifest_dirty.lock().unwrap() = true;

        let should_flush = self.last_flush.lock().unwrap().elapsed() >= AUTO_FLUSH_INTERVAL;
        drop(entries);
        if should_flush {
            self.flush_manifest_inner();
        }
    }

    /// Force an immediate manifest flush regardless of timer.
    pub fn force_flush_manifest(&self) {
        self.flush_manifest_inner();
    }

    /// Load the current session's manifest from disk.
    pub fn load_current_manifest(&self) -> Option<SessionManifest> {
        // Prefer in-memory entries if available (not yet flushed)
        let entries = self.manifest_entries.lock().unwrap();
        if !entries.is_empty() {
            return Some(SessionManifest {
                version: 1,
                timestamp: OffsetDateTime::now_utc(),
                files: entries.clone(),
            });
        }
        drop(entries);
        self.load_manifest_file(&self.manifest_path())
    }

    /// Load the previous session's manifest, if one exists.
    pub fn load_previous_manifest(&self) -> Option<SessionManifest> {
        self.load_manifest_file(&self.previous_manifest_path())
    }

    /// Path to the current session manifest file.
    pub fn manifest_path(&self) -> PathBuf {
        self.cache_dir.join("manifest.json")
    }

    /// Path to the previous session manifest file.
    fn previous_manifest_path(&self) -> PathBuf {
        self.cache_dir.join("manifest.previous.json")
    }

    /// Flush manifest entries to disk. On first flush of a session, rotates
    /// any existing manifest to the previous-manifest slot.
    fn flush_manifest_inner(&self) {
        let entries = self.manifest_entries.lock().unwrap();
        if entries.is_empty() {
            return;
        }

        // Rotate on first flush of this session
        let mut has_rotated = self.has_rotated.lock().unwrap();
        if !*has_rotated {
            let current = self.manifest_path();
            if current.is_file() {
                let prev = self.previous_manifest_path();
                if let Err(e) = fs::rename(&current, &prev) {
                    tracing::warn!("Failed to rotate manifest: {e}");
                }
            }
            *has_rotated = true;
        }
        drop(has_rotated);

        let manifest = SessionManifest {
            version: 1,
            timestamp: OffsetDateTime::now_utc(),
            files: entries.clone(),
        };
        drop(entries);

        let json = match serde_json::to_string_pretty(&manifest) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("Failed to serialize manifest: {e}");
                return;
            }
        };

        // Atomic write via tmp + rename
        let tmp_path = self.cache_dir.join("manifest.tmp.json");
        if let Err(e) = fs::write(&tmp_path, json.as_bytes()) {
            tracing::error!("Failed to write tmp manifest: {e}");
            return;
        }

        let target = self.manifest_path();
        if let Err(e) = fs::rename(&tmp_path, &target) {
            tracing::error!("Failed to rename manifest: {e}");
            return;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&target, fs::Permissions::from_mode(0o600));
        }

        *self.last_flush.lock().unwrap() = Instant::now();
        *self.manifest_dirty.lock().unwrap() = false;
    }

    /// Load and deserialize a manifest file.
    fn load_manifest_file(&self, path: &Path) -> Option<SessionManifest> {
        let data = fs::read_to_string(path).ok()?;
        match serde_json::from_str(&data) {
            Ok(m) => Some(m),
            Err(e) => {
                tracing::warn!("Corrupted manifest at {}: {e}", path.display());
                None
            }
        }
    }

    // ── Existing private methods ─────────────────────────────────────

    /// Remove cache entries for files that no longer exist.
    fn cleanup(&self) {
        let entries = match fs::read_dir(&self.cache_dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        let mut evicted = 0u32;

        for entry in entries.flatten() {
            let filename = entry.file_name();
            let filename = filename.to_string_lossy();

            // Only process .json files
            if !filename.ends_with(".json") {
                continue;
            }

            // Skip manifest files
            if filename.starts_with("manifest") {
                continue;
            }

            // Reverse the encoding: strip .json, replace __ with /
            let rel_path = filename
                .trim_end_matches(".json")
                .replace("__", "/");

            let source_path = self.project_root.join(&rel_path);
            if !source_path.is_file() {
                if let Err(e) = fs::remove_file(entry.path()) {
                    tracing::warn!("Failed to remove stale cache entry {filename}: {e}");
                }
                evicted += 1;
            }
        }

        if evicted > 0 {
            tracing::info!("Skeleton cache cleanup: evicted {evicted} stale entries");
        }
    }

    fn load(&self, file: &str) -> Option<CacheEntry> {
        let path = self.entry_path(file);
        let data = fs::read_to_string(&path).ok()?;
        match serde_json::from_str(&data) {
            Ok(entry) => Some(entry),
            Err(e) => {
                tracing::warn!("Corrupted cache entry for {file}, treating as miss: {e}");
                let _ = fs::remove_file(&path);
                None
            }
        }
    }

    fn entry_path(&self, file: &str) -> PathBuf {
        let key = file.replace('/', "__");
        self.cache_dir.join(format!("{key}.json"))
    }
}

impl Drop for SkeletonCache {
    fn drop(&mut self) {
        if *self.manifest_dirty.lock().unwrap() {
            self.flush_manifest_inner();
        }
    }
}

/// SHA-256 hash of content, returned as 64 hex chars.
pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

/// Extract mtime from file metadata as Unix timestamp (seconds).
pub fn mtime_secs(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
