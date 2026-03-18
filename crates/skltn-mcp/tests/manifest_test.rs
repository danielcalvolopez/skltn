use std::fs;
use tempfile::tempdir;
use skltn_mcp::cache::SkeletonCache;

#[test]
fn test_record_manifest_entry_stores_file() {
    let dir = tempdir().unwrap();
    let cache = SkeletonCache::new(dir.path()).unwrap();

    cache.record_manifest_entry("src/main.rs");

    let manifest = cache.load_current_manifest().unwrap();
    assert_eq!(manifest.files, vec!["src/main.rs"]);
}

#[test]
fn test_manifest_deduplicates_entries() {
    let dir = tempdir().unwrap();
    let cache = SkeletonCache::new(dir.path()).unwrap();

    cache.record_manifest_entry("src/main.rs");
    cache.record_manifest_entry("src/lib.rs");
    cache.record_manifest_entry("src/main.rs");

    let manifest = cache.load_current_manifest().unwrap();
    assert_eq!(manifest.files, vec!["src/main.rs", "src/lib.rs"]);
}

#[test]
fn test_manifest_preserves_insertion_order() {
    let dir = tempdir().unwrap();
    let cache = SkeletonCache::new(dir.path()).unwrap();

    cache.record_manifest_entry("src/c.rs");
    cache.record_manifest_entry("src/a.rs");
    cache.record_manifest_entry("src/b.rs");

    let manifest = cache.load_current_manifest().unwrap();
    assert_eq!(manifest.files, vec!["src/c.rs", "src/a.rs", "src/b.rs"]);
}

#[test]
fn test_load_previous_manifest_returns_none_on_first_session() {
    let dir = tempdir().unwrap();
    let cache = SkeletonCache::new(dir.path()).unwrap();

    assert!(cache.load_previous_manifest().is_none());
}

#[test]
fn test_manifest_rotation_on_first_write() {
    let dir = tempdir().unwrap();

    // Session 1: write some entries
    {
        let cache = SkeletonCache::new(dir.path()).unwrap();
        cache.record_manifest_entry("src/auth.rs");
        cache.record_manifest_entry("src/api.rs");
        cache.force_flush_manifest();
    }

    // Session 2: first write rotates session 1's manifest
    {
        let cache = SkeletonCache::new(dir.path()).unwrap();
        cache.record_manifest_entry("src/new_file.rs");
        cache.force_flush_manifest();

        // Previous manifest has session 1's files
        let prev = cache.load_previous_manifest().unwrap();
        assert_eq!(prev.files, vec!["src/auth.rs", "src/api.rs"]);

        // Current manifest has session 2's files
        let current = cache.load_current_manifest().unwrap();
        assert_eq!(current.files, vec!["src/new_file.rs"]);
    }
}

#[test]
fn test_manifest_flush_is_atomic() {
    let dir = tempdir().unwrap();
    let cache = SkeletonCache::new(dir.path()).unwrap();

    cache.record_manifest_entry("src/main.rs");
    cache.force_flush_manifest();

    // Verify the file exists and is valid JSON
    let manifest_path = cache.manifest_path();
    let content = fs::read_to_string(&manifest_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["version"], 1);
    assert!(parsed["timestamp"].is_string());
    assert_eq!(parsed["files"][0], "src/main.rs");
}
