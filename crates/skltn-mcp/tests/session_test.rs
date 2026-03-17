use std::path::PathBuf;

use skltn_mcp::budget::CacheHint;
use skltn_mcp::session::SessionTracker;

#[test]
fn test_unknown_hint_for_unseen_file() {
    let tracker = SessionTracker::new();
    let hint = tracker.hint_for(&PathBuf::from("/repo/src/main.rs"));
    assert_eq!(hint, CacheHint::Unknown);
}

#[test]
fn test_recently_served_after_record() {
    let mut tracker = SessionTracker::new();
    let path = PathBuf::from("/repo/src/main.rs");
    tracker.record_full(&path);
    let hint = tracker.hint_for(&path);
    assert_eq!(hint, CacheHint::RecentlyServed);
}

#[test]
fn test_different_file_still_unknown() {
    let mut tracker = SessionTracker::new();
    tracker.record_full(&PathBuf::from("/repo/src/main.rs"));
    let hint = tracker.hint_for(&PathBuf::from("/repo/src/lib.rs"));
    assert_eq!(hint, CacheHint::Unknown);
}

#[test]
fn test_multiple_records_same_file() {
    let mut tracker = SessionTracker::new();
    let path = PathBuf::from("/repo/src/main.rs");
    tracker.record_full(&path);
    tracker.record_full(&path);
    let hint = tracker.hint_for(&path);
    assert_eq!(hint, CacheHint::RecentlyServed);
}

#[test]
fn test_multiple_different_files() {
    let mut tracker = SessionTracker::new();
    let path_a = PathBuf::from("/repo/src/a.rs");
    let path_b = PathBuf::from("/repo/src/b.rs");
    tracker.record_full(&path_a);
    tracker.record_full(&path_b);
    assert_eq!(tracker.hint_for(&path_a), CacheHint::RecentlyServed);
    assert_eq!(tracker.hint_for(&path_b), CacheHint::RecentlyServed);
}
