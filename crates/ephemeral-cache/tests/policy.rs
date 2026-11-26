use ephemeral_cache::{EphemeralCache, Source, BULK_TTL};
use std::time::Duration;

fn entries(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir).map_or_else(
        |_| Vec::new(),
        |listing| listing.flatten().map(|item| item.path()).collect(),
    )
}

#[test]
fn fresh_entry_is_served_and_expired_entry_is_refetched() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = EphemeralCache::new(dir.path(), Duration::from_millis(50), 1024);
    cache.put("k", b"value").expect("put");
    assert_eq!(cache.get("k").as_deref(), Some(b"value".as_slice()));
    std::thread::sleep(Duration::from_millis(120));
    assert_eq!(cache.get("k"), None, "expired entry must miss");
    assert!(entries(dir.path()).is_empty(), "expired entry is deleted");
}

#[test]
fn ttl_is_clamped_to_seven_days() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = EphemeralCache::new(dir.path(), Duration::from_hours(2160), 10);
    assert_eq!(cache.ttl(), BULK_TTL);
}

#[test]
fn cap_evicts_oldest_first() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = EphemeralCache::new(dir.path(), BULK_TTL, 10);
    cache.put("a", b"aaaa").expect("a");
    std::thread::sleep(Duration::from_millis(20));
    cache.put("b", b"bbbb").expect("b");
    std::thread::sleep(Duration::from_millis(20));
    cache.put("c", b"cccc").expect("c");
    assert_eq!(cache.get("a"), None, "oldest evicted");
    assert!(cache.get("b").is_some());
    assert!(cache.get("c").is_some());
}

#[test]
fn oversized_value_is_not_stored_and_no_temp_files_remain() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = EphemeralCache::new(dir.path(), BULK_TTL, 4);
    cache.put("big", b"too large").expect("put");
    assert_eq!(cache.get("big"), None);
    cache.put("ok", b"ok").expect("put");
    assert!(entries(dir.path())
        .iter()
        .all(|path| path.to_string_lossy().ends_with(".entry")));
}

#[test]
fn source_names_match_measurement_values() {
    assert_eq!(Source::Api.as_str(), "api");
    assert_eq!(Source::Stream.as_str(), "stream");
    assert_eq!(Source::Cache.as_str(), "cache");
}
