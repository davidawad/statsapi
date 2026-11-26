use ephemeral_cache::{EphemeralCache, Source, Trail, LIVE_TTL};
use std::io;
use std::time::Duration;

#[test]
fn live_preferred_cache_only_on_failure_and_trail_reports_least_live() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = EphemeralCache::new(dir.path(), LIVE_TTL, 1024);
    let live = cache
        .answer("k", false, || Ok::<_, io::Error>(b"live".to_vec()))
        .expect("live");
    assert_eq!(live.1, Source::Api);
    let down = cache
        .answer("k", false, || Err::<Vec<u8>, _>(io::Error::other("down")))
        .expect("fallback");
    assert_eq!(
        (down.0.as_slice(), down.1),
        (b"live".as_slice(), Source::Cache)
    );
    let trail = Trail::new();
    assert_eq!(trail.answered(), None);
    trail.note(Source::Api);
    trail.note(Source::Cache);
    trail.note(Source::Api);
    assert_eq!(trail.take(), Some(Source::Cache));
    assert_eq!(trail.answered(), None);
}

#[test]
fn expired_entry_is_refetched_live() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = EphemeralCache::new(dir.path(), Duration::ZERO, 1024);
    cache.put("k", b"old").expect("stored");
    let (body, source) = cache
        .answer("k", false, || Ok::<_, io::Error>(b"new".to_vec()))
        .expect("live");
    assert_eq!((body.as_slice(), source), (b"new".as_slice(), Source::Api));
}
