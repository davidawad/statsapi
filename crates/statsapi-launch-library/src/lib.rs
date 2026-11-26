//! Blocking client for The Space Devs Launch Library 2 (`ll.thespacedevs.com`).
//!
//! The free tier allows about 15 requests per hour, so the client asks the
//! API for counts (`limit=1`) instead of paging launches. A caller may give a
//! cache directory; it is used only as a last resort, when the live API is
//! throttled or unreachable, and entries expire after a bounded TTL.

use reqwest::blocking::Client as HttpClient;
use reqwest::header::RETRY_AFTER;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime};
use thiserror::Error;

/// Production API root.
pub const DEFAULT_BASE_URL: &str = "https://ll.thespacedevs.com/2.2.0";

/// Longest cache TTL the client honours, whatever the caller asks for.
pub const MAX_CACHE_TTL: Duration = Duration::from_hours(168);

/// Total bytes the fallback cache may hold before the oldest entries are pruned.
const CACHE_CAP_BYTES: u64 = 4 * 1024 * 1024;

/// Agency pages `providers` reads before stopping.
pub const MAX_AGENCY_PAGES: usize = 5;

/// Longest `Retry-After` the client sleeps through; longer throttles fail fast.
const MAX_RETRY_WAIT: Duration = Duration::from_secs(5);

/// Launch Library status ids for launches that have not flown yet: Go, TBD, Hold, TBC.
///
/// The upcoming list also keeps recently flown launches, so `next_launch` filters on these.
pub const STATUS_NOT_YET_LAUNCHED: [u32; 4] = [1, 2, 5, 8];
/// Launch Library status ids: success.
pub const STATUS_SUCCESS: u32 = 3;
/// Launch Library status ids: failure, partial failure.
pub const STATUS_FAILURES: [u32; 2] = [4, 7];

/// Errors returned by requests, decoding, or cache access.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// The HTTP request failed after retrying transient errors.
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    /// The upstream returned a non-success response.
    #[error("upstream returned HTTP status {0}")]
    HttpStatus(u16),
    /// The upstream response did not match its documented layout.
    #[error("invalid upstream response: {0}")]
    InvalidResponse(String),
    /// Offline mode is on and the cache has no fresh answer.
    #[error("offline and no fresh cached answer")]
    Offline,
    /// The cache could not be read or written.
    #[error("cache I/O failed: {0}")]
    Cache(#[from] std::io::Error),
}

/// Result type returned by this client.
pub type Result<T> = std::result::Result<T, Error>;

/// Where an answer came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Origin {
    /// A live API response.
    Api,
    /// The TTL-bounded fallback cache, used because the live API failed.
    Cache,
}

impl Origin {
    /// The `measurement.source` word for this origin.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Cache => "cache",
        }
    }
}

/// Which launch list a query reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Listing {
    /// Launches that have already happened (`/launch/previous/`).
    Previous,
    /// Launches still to come (`/launch/upcoming/`).
    Upcoming,
}

impl Listing {
    fn path(self) -> &'static str {
        match self {
            Self::Previous => "launch/previous/",
            Self::Upcoming => "launch/upcoming/",
        }
    }
}

/// Filters for a launch query. Dates are `YYYY-MM-DD` (UTC, inclusive start).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Filter {
    /// Launch service provider agency ids (`lsp__ids`); empty keeps every provider.
    pub provider_ids: Vec<u32>,
    /// Earliest net launch date (`net__gte`).
    pub net_from: Option<String>,
    /// Status ids to keep (`status__ids`); empty keeps every status.
    pub status_ids: Vec<u32>,
}

/// A launch count and the URL that produced it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Counted {
    /// Number of launches matching the filter.
    pub count: u64,
    /// Request URL.
    pub source_url: String,
    /// Whether the number is live or cached.
    pub origin: Origin,
}

/// Agencies whose country is exactly one given country.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Providers {
    /// Agency ids, ascending.
    pub ids: Vec<u32>,
    /// First request URL.
    pub source_url: String,
    /// Live, or cached when any page came from the cache.
    pub origin: Origin,
}

/// The next upcoming launch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NextLaunch {
    /// Launch name, for example `Falcon 9 Block 5 | Starlink`.
    pub name: String,
    /// Net launch time as published (RFC 3339).
    pub net: String,
    /// Request URL.
    pub source_url: String,
    /// Whether the answer is live or cached.
    pub origin: Origin,
}

/// Blocking Launch Library client.
#[derive(Clone, Debug)]
pub struct Client {
    http: HttpClient,
    base_url: String,
    cache: Option<(PathBuf, Duration)>,
    offline: bool,
    max_attempts: u32,
    backoff_base_ms: u64,
}

/// Builder for [`Client`].
#[derive(Clone, Debug)]
pub struct ClientBuilder {
    user_agent: String,
    base_url: String,
    cache: Option<(PathBuf, Duration)>,
    offline: bool,
    max_attempts: u32,
    backoff_base_ms: u64,
}

impl ClientBuilder {
    /// Overrides the API root (tests, mirrors).
    #[must_use]
    pub fn base_url(mut self, base_url: &str) -> Self {
        base_url
            .trim_end_matches('/')
            .clone_into(&mut self.base_url);
        self
    }

    /// Enables the last-resort cache in `dir`; `ttl` is clamped to [`MAX_CACHE_TTL`].
    #[must_use]
    pub fn fallback_cache(mut self, dir: impl Into<PathBuf>, ttl: Duration) -> Self {
        self.cache = Some((dir.into(), ttl.min(MAX_CACHE_TTL)));
        self
    }

    /// Reads only the fallback cache and never contacts the API.
    #[must_use]
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    /// Sets the attempts per request (at least one) and the first backoff step.
    #[must_use]
    pub fn retries(mut self, max_attempts: u32, backoff_base: Duration) -> Self {
        self.max_attempts = max_attempts.max(1);
        self.backoff_base_ms = u64::try_from(backoff_base.as_millis()).unwrap_or(u64::MAX);
        self
    }

    /// Builds the client.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be constructed.
    pub fn build(self) -> Result<Client> {
        let http = HttpClient::builder()
            .user_agent(self.user_agent)
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Client {
            http,
            base_url: self.base_url,
            cache: self.cache,
            offline: self.offline,
            max_attempts: self.max_attempts,
            backoff_base_ms: self.backoff_base_ms,
        })
    }
}

impl Client {
    /// Starts a builder with a descriptive User-Agent.
    #[must_use]
    pub fn builder(user_agent: &str) -> ClientBuilder {
        ClientBuilder {
            user_agent: user_agent.to_owned(),
            base_url: DEFAULT_BASE_URL.to_owned(),
            cache: None,
            offline: false,
            max_attempts: 3,
            backoff_base_ms: 500,
        }
    }

    /// Counts launches in `listing` matching `filter`.
    ///
    /// # Errors
    /// Returns an error when the live API and the fallback cache both fail, or
    /// the response has no `count`.
    pub fn count(&self, listing: Listing, filter: &Filter) -> Result<Counted> {
        let url = self.url(listing, filter, 1);
        let (body, origin) = self.get_json(&url)?;
        let count = body
            .get("count")
            .and_then(Value::as_u64)
            .ok_or_else(|| Error::InvalidResponse("response has no numeric count".into()))?;
        Ok(Counted {
            count,
            source_url: url,
            origin,
        })
    }

    /// The soonest launch that has not flown yet, or `None` when nothing is scheduled.
    ///
    /// # Errors
    /// Returns an error when the live API and the fallback cache both fail, or
    /// a launch lacks a name or `net`.
    pub fn next_launch(&self, provider_ids: &[u32]) -> Result<Option<NextLaunch>> {
        let filter = Filter {
            provider_ids: provider_ids.to_vec(),
            status_ids: STATUS_NOT_YET_LAUNCHED.to_vec(),
            ..Filter::default()
        };
        let url = self.url(Listing::Upcoming, &filter, 1);
        let (body, origin) = self.get_json(&url)?;
        let Some(first) = body
            .get("results")
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
        else {
            return Ok(None);
        };
        let field = |key: &str| {
            first
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| Error::InvalidResponse(format!("launch has no {key}")))
        };
        Ok(Some(NextLaunch {
            name: field("name")?,
            net: field("net")?,
            source_url: url,
            origin,
        }))
    }

    /// Agencies whose `country_code` is exactly `country` (ISO 3166-1 alpha-3).
    ///
    /// Multinational bodies list several codes and are excluded. Reads up to
    /// [`MAX_AGENCY_PAGES`] pages of 100 agencies.
    ///
    /// # Errors
    /// Returns an error when the live API and the fallback cache both fail, or
    /// a page lacks `results`.
    pub fn providers(&self, country: &str) -> Result<Providers> {
        let first = format!(
            "{}/agencies/?limit=100&mode=list&country_code={country}",
            self.base_url
        );
        let mut ids = Vec::new();
        let mut origin = Origin::Api;
        let mut url = first.clone();
        for _ in 0..MAX_AGENCY_PAGES {
            let (body, page_origin) = self.get_json(&url)?;
            if page_origin == Origin::Cache {
                origin = Origin::Cache;
            }
            let rows = body
                .get("results")
                .and_then(Value::as_array)
                .ok_or_else(|| Error::InvalidResponse("agency page has no results".into()))?;
            for row in rows {
                let exact = row.get("country_code").and_then(Value::as_str) == Some(country);
                let id = row
                    .get("id")
                    .and_then(Value::as_u64)
                    .and_then(|id| u32::try_from(id).ok());
                if let (true, Some(id)) = (exact, id) {
                    ids.push(id);
                }
            }
            match body.get("next").and_then(Value::as_str) {
                Some(next) => next.clone_into(&mut url),
                None => break,
            }
        }
        ids.sort_unstable();
        ids.dedup();
        Ok(Providers {
            ids,
            source_url: first,
            origin,
        })
    }

    fn url(&self, listing: Listing, filter: &Filter, limit: u32) -> String {
        let mut url = format!(
            "{}/{}?limit={limit}&mode=list",
            self.base_url,
            listing.path()
        );
        if !filter.provider_ids.is_empty() {
            let ids: Vec<String> = filter.provider_ids.iter().map(u32::to_string).collect();
            let _ = write!(url, "&lsp__ids={}", ids.join(","));
        }
        if let Some(from) = &filter.net_from {
            let _ = write!(url, "&net__gte={from}");
        }
        if !filter.status_ids.is_empty() {
            let ids: Vec<String> = filter.status_ids.iter().map(u32::to_string).collect();
            let _ = write!(url, "&status__ids={}", ids.join(","));
        }
        url
    }

    fn get_json(&self, url: &str) -> Result<(Value, Origin)> {
        let live = if self.offline {
            Err(Error::Offline)
        } else {
            self.get_live(url)
        };
        match live {
            Ok(bytes) => {
                let body = decode(&bytes)?;
                self.store(url, &bytes);
                Ok((body, Origin::Api))
            }
            Err(live) => match self.load(url) {
                Some(bytes) => Ok((decode(&bytes)?, Origin::Cache)),
                None => Err(live),
            },
        }
    }

    fn get_live(&self, url: &str) -> Result<Vec<u8>> {
        let mut last = None;
        for attempt in 0..self.max_attempts {
            let mut wait = None;
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    return Ok(response.bytes()?.to_vec());
                }
                Ok(response) => {
                    let status = response.status();
                    wait = response
                        .headers()
                        .get(RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.parse::<u64>().ok())
                        .map(Duration::from_secs);
                    if !(status.as_u16() == 429 || status.is_server_error()) {
                        return Err(Error::HttpStatus(status.as_u16()));
                    }
                    last = Some(Error::HttpStatus(status.as_u16()));
                }
                Err(error) => last = Some(Error::Request(error)),
            }
            if wait.is_some_and(|wait| wait > MAX_RETRY_WAIT) {
                break;
            }
            if attempt + 1 < self.max_attempts {
                let base = self.backoff_base_ms.saturating_mul(1 << attempt.min(8));
                let jitter = fastrand::u64(0..=base);
                thread::sleep(wait.unwrap_or(Duration::from_millis(base + jitter)));
            }
        }
        Err(last.unwrap_or_else(|| Error::InvalidResponse("no attempts configured".into())))
    }

    fn entry(&self, url: &str) -> Option<(PathBuf, Duration)> {
        let (dir, ttl) = self.cache.as_ref()?;
        let digest = Sha256::digest(url.as_bytes());
        Some((dir.join(format!("{digest:x}.json")), *ttl))
    }

    fn load(&self, url: &str) -> Option<Vec<u8>> {
        let (path, ttl) = self.entry(url)?;
        let age = SystemTime::now()
            .duration_since(fs::metadata(&path).ok()?.modified().ok()?)
            .ok()?;
        (age <= ttl).then(|| fs::read(path).ok()).flatten()
    }

    /// Best-effort: a cache write failure never fails a live answer.
    fn store(&self, url: &str, bytes: &[u8]) {
        let Some((path, ttl)) = self.entry(url) else {
            return;
        };
        let Some(dir) = path.parent() else { return };
        if fs::create_dir_all(dir).is_err() {
            return;
        }
        let temporary = path.with_extension(format!("{}.tmp", fastrand::u64(..)));
        if fs::write(&temporary, bytes).is_ok() && fs::rename(&temporary, &path).is_err() {
            let _ = fs::remove_file(&temporary);
        }
        prune(dir, ttl);
    }
}

fn decode(bytes: &[u8]) -> Result<Value> {
    serde_json::from_slice(bytes).map_err(|error| Error::InvalidResponse(error.to_string()))
}

/// Removes expired entries, then the oldest until the directory fits the cap.
fn prune(dir: &Path, ttl: Duration) {
    let Ok(read) = fs::read_dir(dir) else { return };
    let now = SystemTime::now();
    let mut files: Vec<(PathBuf, u64, SystemTime)> = read
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            Some((entry.path(), meta.len(), meta.modified().ok()?))
        })
        .collect();
    files.sort_by_key(|(_, _, modified)| *modified);
    let mut total: u64 = files.iter().map(|(_, size, _)| *size).sum();
    for (path, size, modified) in files {
        let expired = now.duration_since(modified).unwrap_or_default() > ttl;
        if (expired || total > CACHE_CAP_BYTES) && fs::remove_file(path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
}
