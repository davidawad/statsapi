//! Last-resort ephemeral cache for adapter source access.
//!
//! Source-access order is: (1) the live API, (2) a streamed download that is
//! filtered on the fly and persists nothing, (3) only then this cache. Entries
//! expire after a TTL (live responses: minutes to hours; bulk or vintage
//! extracts: at most [`BULK_TTL`]), the directory is bounded by a byte cap with
//! oldest-first eviction, and bulk archives are never stored: callers cache the
//! extract they computed, not the download.
//!
//! There are no locks. Writes go to a temporary file and are renamed into place,
//! so readers see a whole entry or none; concurrent writers last-write-win.

mod flight;
mod sweep;
pub use flight::Flight;
pub use sweep::{describe, stamp_records, sweep_dir, Cache, Policy, Probe, MAX_BULK_TTL};

/// Alias used by adapters stamping `measurement.source_path`.
pub type AccessPath = Source;

use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Longest permitted TTL for bulk or vintage extracts.
pub const BULK_TTL: Duration = Duration::from_hours(168);
/// Default TTL for live API responses.
pub const LIVE_TTL: Duration = Duration::from_hours(1);
/// Default per-adapter size cap.
pub const DEFAULT_CAP_BYTES: u64 = 256 * 1024 * 1024;

const SUFFIX: &str = ".entry";

/// Path that answered a request; recorded as `measurement.source`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Source {
    /// Live API answered.
    Api,
    /// Streamed download, filtered on the fly, nothing persisted.
    #[default]
    Stream,
    /// Fresh entry from the ephemeral cache.
    Cache,
}

impl Source {
    /// Stable lowercase name: `api`, `stream` or `cache`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Stream => "stream",
            Self::Cache => "cache",
        }
    }
}

/// A directory of TTL-bounded entries with a total size cap.
#[derive(Clone, Debug)]
pub struct EphemeralCache {
    dir: PathBuf,
    ttl: Duration,
    cap_bytes: u64,
}

impl EphemeralCache {
    /// Creates a cache; `ttl` is clamped to [`BULK_TTL`].
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>, ttl: Duration, cap_bytes: u64) -> Self {
        Self {
            dir: dir.into(),
            ttl: ttl.min(BULK_TTL),
            cap_bytes,
        }
    }

    /// Creates a bulk-extract cache with the maximum TTL and default cap.
    #[must_use]
    pub fn bulk(dir: impl Into<PathBuf>) -> Self {
        Self::new(dir, BULK_TTL, DEFAULT_CAP_BYTES)
    }

    /// Returns the effective TTL.
    #[must_use]
    pub const fn ttl(&self) -> Duration {
        self.ttl
    }

    /// Returns the size cap in bytes.
    #[must_use]
    pub const fn cap_bytes(&self) -> u64 {
        self.cap_bytes
    }

    fn path(&self, key: &str) -> PathBuf {
        let digest = Sha256::digest(key.as_bytes());
        let mut name = String::with_capacity(64 + SUFFIX.len());
        for byte in digest {
            let _ = write!(name, "{byte:02x}");
        }
        name.push_str(SUFFIX);
        self.dir.join(name)
    }

    /// Returns a fresh entry, or `None` when absent or expired. An expired
    /// entry is deleted so the caller refetches.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.get_at(key, SystemTime::now())
    }

    fn get_at(&self, key: &str, now: SystemTime) -> Option<Vec<u8>> {
        let path = self.path(key);
        let modified = fs::metadata(&path).ok()?.modified().ok()?;
        if now
            .duration_since(modified)
            .is_ok_and(|age| age >= self.ttl)
        {
            let _ = fs::remove_file(&path);
            return None;
        }
        fs::read(&path).ok()
    }

    /// Stores an entry, then evicts expired and oldest entries until the
    /// directory fits the cap. A value larger than the cap is not stored.
    ///
    /// # Errors
    /// Returns the I/O error from creating the directory or writing the entry.
    pub fn put(&self, key: &str, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() as u64 > self.cap_bytes {
            return Ok(());
        }
        fs::create_dir_all(&self.dir)?;
        let path = self.path(key);
        let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
        fs::write(&temporary, bytes)?;
        if let Err(error) = fs::rename(&temporary, &path) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        self.evict(&path);
        Ok(())
    }

    /// Removes expired entries, then the oldest until within the cap. The entry
    /// at `keep` is evicted last.
    fn evict(&self, keep: &Path) {
        let now = SystemTime::now();
        let Ok(listing) = fs::read_dir(&self.dir) else {
            return;
        };
        let mut live = Vec::new();
        for item in listing.flatten() {
            let path = item.path();
            if !path.to_string_lossy().ends_with(SUFFIX) {
                continue;
            }
            let Ok(meta) = item.metadata() else { continue };
            let modified = meta.modified().unwrap_or(now);
            if now
                .duration_since(modified)
                .is_ok_and(|age| age >= self.ttl)
            {
                let _ = fs::remove_file(&path);
            } else {
                live.push((path == keep, modified, meta.len(), path));
            }
        }
        let mut total: u64 = live.iter().map(|entry| entry.2).sum();
        live.sort();
        for (_, _, len, path) in live {
            if total <= self.cap_bytes {
                break;
            }
            if fs::remove_file(&path).is_ok() || !path.exists() {
                total = total.saturating_sub(len);
            }
        }
    }
}

impl Source {
    fn code(self) -> u8 {
        match self {
            Self::Api => 1,
            Self::Stream => 2,
            Self::Cache => 3,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Api),
            2 => Some(Self::Stream),
            3 => Some(Self::Cache),
            _ => None,
        }
    }
}

/// Shared record of the access paths a client used, cloneable into adapters.
///
/// The least-live path wins: one cache hit makes the whole answer `cache`.
#[derive(Clone, Debug, Default)]
pub struct Trail(std::sync::Arc<std::sync::atomic::AtomicU8>);

impl Trail {
    /// A trail with nothing recorded.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `source` answered one request.
    pub fn note(&self, source: Source) {
        self.0
            .fetch_max(source.code(), std::sync::atomic::Ordering::Relaxed);
    }

    /// Return the least-live path noted so far and clear the trail.
    #[must_use]
    pub fn take(&self) -> Option<Source> {
        Source::from_code(self.0.swap(0, std::sync::atomic::Ordering::Relaxed))
    }

    /// The least-live path noted so far, if any.
    #[must_use]
    pub fn answered(&self) -> Option<Source> {
        Source::from_code(self.0.load(std::sync::atomic::Ordering::Relaxed))
    }
}

impl EphemeralCache {
    /// Answer `key` live first, falling back to a fresh entry only when the
    /// live request fails or `cache_only` is set. A live answer is stored.
    ///
    /// # Errors
    /// Returns the live error when there is no fresh entry to fall back to.
    pub fn answer<E>(
        &self,
        key: &str,
        cache_only: bool,
        live: impl FnOnce() -> Result<Vec<u8>, E>,
    ) -> Result<(Vec<u8>, Source), E>
    where
        E: From<io::Error>,
    {
        if cache_only {
            return self
                .get(key)
                .map(|body| (body, Source::Cache))
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, format!("cache miss for {key}")).into()
                });
        }
        match live() {
            Ok(body) => {
                let _ = self.put(key, &body);
                Ok((body, Source::Api))
            }
            Err(error) => self.get(key).map(|body| (body, Source::Cache)).ok_or(error),
        }
    }
}

/// The path an observation took (alternate spelling of [`Source`]).
pub type AnswerPath = Source;
/// Longest TTL for a bulk or vintage extract (alias of [`BULK_TTL`]).
pub const MAX_EXTRACT_TTL: Duration = BULK_TTL;

impl EphemeralCache {
    /// A cache rooted at `dir` with `policy`.
    #[must_use]
    pub fn with_policy(dir: impl Into<PathBuf>, policy: Policy) -> Self {
        Self::new(dir, policy.ttl, policy.cap_bytes)
    }

    /// Answer from `live` (recorded as `live_path`), else from a fresh entry
    /// ([`Source::Cache`]). A live answer is stored as the fallback.
    ///
    /// # Errors
    ///
    /// The live error when it failed and no fresh entry exists.
    pub fn resolve<E>(
        &self,
        key: &str,
        live_path: Source,
        live: impl FnOnce() -> Result<Vec<u8>, E>,
    ) -> Result<(Vec<u8>, Source), E> {
        match live() {
            Ok(body) => {
                let _ = self.put(key, &body);
                Ok((body, live_path))
            }
            Err(error) => self.get(key).map(|body| (body, Source::Cache)).ok_or(error),
        }
    }
}

/// Write `bytes` to `path` through a unique temp file and `rename`.
///
/// # Errors
///
/// The I/O error when the directory, temp file or rename fails.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("cache path has no parent"))?;
    fs::create_dir_all(dir)?;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let temp = dir.join(format!(
        ".tmp-{}-{nanos}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&temp, bytes)?;
    fs::rename(&temp, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })
}

/// Delete the oldest files under `root` (by modification time, recursively)
/// until the total size is at most `cap_bytes`. A missing `root` is fine.
///
/// # Errors
///
/// The I/O error when the directory cannot be listed.
pub fn evict_oldest(root: &Path, cap_bytes: u64) -> io::Result<()> {
    fn collect(dir: &Path, out: &mut Vec<(PathBuf, u64, SystemTime)>) -> io::Result<()> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        for entry in entries {
            let entry = entry?;
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                collect(&entry.path(), out)?;
            } else if let Ok(modified) = meta.modified() {
                out.push((entry.path(), meta.len(), modified));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    collect(root, &mut files)?;
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= cap_bytes {
        return Ok(());
    }
    files.sort_by_key(|f| f.2);
    for (path, size, _) in files {
        if total <= cap_bytes {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
    Ok(())
}

#[cfg(test)]
mod dot_zmn92_tests {
    use super::*;

    #[test]
    fn resolve_prefers_live_then_a_fresh_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = EphemeralCache::with_policy(
            dir.path(),
            Policy {
                ttl: Duration::from_secs(60),
                cap_bytes: 1000,
            },
        );
        let live = cache.resolve::<&str>("k", Source::Stream, || Ok(b"x".to_vec()));
        assert_eq!(live, Ok((b"x".to_vec(), Source::Stream)));
        let fallback = cache.resolve("k", Source::Stream, || Err("down"));
        assert_eq!(fallback, Ok((b"x".to_vec(), Source::Cache)));
    }

    #[test]
    fn evict_oldest_walks_subdirectories_and_tolerates_a_missing_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        evict_oldest(&dir.path().join("absent"), 0).expect("missing root");
        write_atomic(&dir.path().join("sub/x"), &[0; 10]).expect("write");
        evict_oldest(dir.path(), 5).expect("evict");
        assert!(!dir.path().join("sub/x").exists());
    }
}
