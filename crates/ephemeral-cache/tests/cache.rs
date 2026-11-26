#![allow(clippy::expect_used)] // Fixture failures should fail the test immediately.

use ephemeral_cache::{sweep_dir, AccessPath, Cache, Policy, Probe};
use std::fs;
use std::time::{Duration, SystemTime};

fn age(path: &std::path::Path, by: Duration) {
    let file = fs::File::options().write(true).open(path).expect("open");
    file.set_modified(SystemTime::now() - by)
        .expect("set mtime");
}

#[test]
fn fresh_entry_is_served_and_expired_entry_is_refetched() {
    let dir = tempfile::tempdir().expect("dir");
    let cache = Cache::new(dir.path(), Policy::live(10, 1 << 20));
    cache.put("k", b"body").expect("put");
    assert_eq!(cache.get("k").as_deref(), Some(&b"body"[..]));
    age(&dir.path().join("k"), Duration::from_mins(11));
    assert_eq!(cache.get("k"), None, "expired entry must miss");
    assert!(!dir.path().join("k").exists(), "expired entry is removed");
}

#[test]
fn cap_evicts_oldest_first() {
    let dir = tempfile::tempdir().expect("dir");
    let cache = Cache::new(dir.path(), Policy::live(60, 10));
    cache.put("old", b"123456").expect("put");
    age(&dir.path().join("old"), Duration::from_secs(60));
    cache.put("new", b"abcdef").expect("put");
    assert_eq!(cache.get("old"), None, "oldest evicted over the cap");
    assert_eq!(cache.get("new").as_deref(), Some(&b"abcdef"[..]));
}

#[test]
fn sweep_dir_bounds_foreign_files_and_bulk_ttl_is_clamped() {
    let dir = tempfile::tempdir().expect("dir");
    fs::write(dir.path().join("a.json"), b"x").expect("write");
    age(&dir.path().join("a.json"), Duration::from_hours(2));
    assert_eq!(sweep_dir(dir.path(), Policy::live(60, 100)), 1);
    assert!(Policy::bulk(10_000, 1).ttl <= ephemeral_cache::MAX_BULK_TTL);
}

#[test]
fn probe_distinguishes_cache_hit_from_live_fetch() {
    let dir = tempfile::tempdir().expect("dir");
    fs::write(dir.path().join("a"), b"x").expect("write");
    let hit = Probe::start(dir.path());
    assert_eq!(hit.path(), AccessPath::Cache);
    let miss = Probe::start(dir.path());
    fs::write(dir.path().join("b"), b"y").expect("write");
    assert_eq!(miss.path(), AccessPath::Api);
    assert_eq!(AccessPath::Stream.as_str(), "stream");
}
