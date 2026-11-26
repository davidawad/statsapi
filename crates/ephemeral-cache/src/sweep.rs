//! TTL sweeping, access-path probing and descriptor helpers shared by adapters whose HTTP
//! clients own their cache files.
//!
//! Source-access order: (1) the live API, (2) a streamed download that is filtered on the fly
//! and persists nothing, (3) only as a last resort this cache. Entries expire after a TTL
//! (minutes to hours for live responses, at most [`MAX_BULK_TTL`] for bulk or vintage
//! extracts) and the directory is held under a byte cap by evicting the oldest entries first.
//! Bulk archives are never kept: extract what is needed, then discard the archive.
//!
//! The cache takes no locks. Writers publish with a temp file plus rename, so a reader sees
//! either the whole entry or none of it; concurrent sweeps tolerate files vanishing.

use crate::Source;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Longest TTL allowed for bulk or vintage extracts.
pub const MAX_BULK_TTL: Duration = Duration::from_hours(MAX_BULK_TTL_HOURS);
const MAX_BULK_TTL_HOURS: u64 = 168;

/// Time-to-live and size cap for one adapter's cache directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub ttl: Duration,
    pub cap_bytes: u64,
}

impl Policy {
    /// Policy for live API responses: a TTL in minutes and a cap in bytes.
    #[must_use]
    pub const fn live(ttl_minutes: u64, cap_bytes: u64) -> Self {
        Self {
            ttl: Duration::from_mins(ttl_minutes),
            cap_bytes,
        }
    }

    /// Policy for bulk or vintage extracts: a TTL in hours, clamped to [`MAX_BULK_TTL`].
    #[must_use]
    pub const fn bulk(ttl_hours: u64, cap_bytes: u64) -> Self {
        let ttl_hours = if ttl_hours > MAX_BULK_TTL_HOURS {
            MAX_BULK_TTL_HOURS
        } else {
            ttl_hours
        };
        Self {
            ttl: Duration::from_hours(ttl_hours),
            cap_bytes,
        }
    }
}

/// A TTL- and cap-bounded directory of opaque entries.
#[derive(Clone, Debug)]
pub struct Cache {
    dir: PathBuf,
    policy: Policy,
}

struct Entry {
    path: PathBuf,
    modified: SystemTime,
    len: u64,
}

fn entries(dir: &Path) -> Vec<Entry> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        let Ok(read) = fs::read_dir(&next) else {
            continue;
        };
        for item in read.flatten() {
            let path = item.path();
            let Ok(meta) = item.metadata() else { continue };
            if meta.is_dir() {
                pending.push(path);
            } else if let Ok(modified) = meta.modified() {
                found.push(Entry {
                    path,
                    modified,
                    len: meta.len(),
                });
            }
        }
    }
    found
}

fn expired(modified: SystemTime, ttl: Duration, now: SystemTime) -> bool {
    now.duration_since(modified).is_ok_and(|age| age > ttl)
}

impl Cache {
    /// Cache rooted at `dir`; the directory is created on first write.
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>, policy: Policy) -> Self {
        Self {
            dir: dir.into(),
            policy,
        }
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    #[must_use]
    pub const fn policy(&self) -> Policy {
        self.policy
    }

    fn entry_path(&self, key: &str) -> PathBuf {
        let safe: String = key
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.dir.join(safe)
    }

    /// The entry for `key` when it exists and has not outlived the TTL.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        let path = self.entry_path(key);
        let modified = fs::metadata(&path).ok()?.modified().ok()?;
        if expired(modified, self.policy.ttl, SystemTime::now()) {
            let _ = fs::remove_file(&path);
            return None;
        }
        fs::read(path).ok()
    }

    /// Stores `bytes` under `key` (temp file plus rename), then enforces the size cap.
    ///
    /// # Errors
    ///
    /// Returns the I/O error when the directory or entry cannot be written.
    pub fn put(&self, key: &str, bytes: &[u8]) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let path = self.entry_path(key);
        let temp = self
            .dir
            .join(format!(".tmp-{}-{}", std::process::id(), key.len()))
            .with_extension(format!("{:x}", bytes.len()));
        let mut file = fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        self.sweep();
        Ok(())
    }

    /// Removes expired entries, then evicts oldest-first until the directory fits the cap.
    /// Returns the number of files removed.
    pub fn sweep(&self) -> usize {
        sweep_dir(&self.dir, self.policy)
    }
}

/// Applies `policy` to every file under `dir`: drops entries older than the TTL, then evicts
/// the oldest remaining files until the total size is within the cap. Works on directories
/// written by any client, so adapters whose HTTP client owns its own cache files still honor
/// the TTL and cap. Returns the number of files removed.
pub fn sweep_dir(dir: &Path, policy: Policy) -> usize {
    let now = SystemTime::now();
    let mut removed = 0;
    let mut kept = Vec::new();
    for entry in entries(dir) {
        if expired(entry.modified, policy.ttl, now) && fs::remove_file(&entry.path).is_ok() {
            removed += 1;
        } else {
            kept.push(entry);
        }
    }
    kept.sort_by_key(|entry| entry.modified);
    let mut total: u64 = kept.iter().map(|entry| entry.len).sum();
    for entry in kept {
        if total <= policy.cap_bytes {
            break;
        }
        if fs::remove_file(&entry.path).is_ok() {
            removed += 1;
        }
        total = total.saturating_sub(entry.len);
    }
    removed
}

/// Detects whether a client call wrote anything to a cache directory, which tells a live
/// fetch (`api`) apart from a cache hit (`cache`).
#[derive(Debug)]
pub struct Probe {
    dir: PathBuf,
    before: Vec<(PathBuf, SystemTime, u64)>,
}

impl Probe {
    /// Snapshots `dir` before the client runs.
    #[must_use]
    pub fn start(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            before: Self::snapshot(dir),
        }
    }

    fn snapshot(dir: &Path) -> Vec<(PathBuf, SystemTime, u64)> {
        let mut files: Vec<_> = entries(dir)
            .into_iter()
            .map(|entry| (entry.path, entry.modified, entry.len))
            .collect();
        files.sort();
        files
    }

    /// `Api` when a file appeared or changed since [`Probe::start`], else `Cache`.
    #[must_use]
    pub fn path(&self) -> Source {
        if Self::snapshot(&self.dir) == self.before {
            Source::Cache
        } else {
            Source::Api
        }
    }
}

/// Descriptor fragment documenting an adapter's access order, TTL and cap.
#[must_use]
pub fn describe(policy: Policy) -> serde_json::Value {
    serde_json::json!({
        "access_order": ["api", "stream", "cache"],
        "ttl_s": policy.ttl.as_secs(),
        "cap_bytes": policy.cap_bytes,
        "eviction": "oldest-first",
        "archives_kept": false,
    })
}

/// Sets `measurement.source_path` on every record in `records` (an object keyed by id).
pub fn stamp_records(records: &mut serde_json::Map<String, serde_json::Value>, path: Source) {
    for record in records.values_mut() {
        if let Some(measurement) = record
            .get_mut("measurement")
            .and_then(|m| m.as_object_mut())
        {
            measurement.insert("source_path".into(), path.as_str().into());
        }
    }
}
