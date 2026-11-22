//! Blocking access to GDELT DOC 2.0 timeline data.

use reqwest::StatusCode;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const API: &str = "https://api.gdeltproject.org/api/v2/doc/doc";
const MAX_ATTEMPTS: u32 = 4;
const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(6);
static LAST_REQUEST: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

/// Errors returned while building, fetching, caching, or decoding a timeline.
#[derive(Debug)]
pub enum Error {
    /// The request URL could not be constructed.
    Url(String),
    /// The HTTP request failed after retries.
    Http(reqwest::Error),
    /// GDELT returned a retryable or unsuccessful HTTP status.
    Status(StatusCode),
    /// The response body could not be decoded as JSON.
    Json(serde_json::Error),
    /// The response did not contain a GDELT timeline.
    MissingTimeline,
    /// A cache read or write failed.
    Cache(std::io::Error),
    /// The process-wide request pacing lock was poisoned.
    PacingPoisoned,
    /// GDELT returned its plain-text rate-limit response.
    RateLimited(String),
    /// Offline mode was requested and the response was not in the cache.
    OfflineCacheMiss,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url(error) => write!(formatter, "GDELT URL error: {error}"),
            Self::Http(error) => write!(formatter, "GDELT HTTP error: {error}"),
            Self::Status(status) => write!(formatter, "GDELT HTTP status: {status}"),
            Self::Json(error) => write!(formatter, "GDELT JSON error: {error}"),
            Self::MissingTimeline => formatter.write_str("GDELT response has no timeline data"),
            Self::Cache(error) => write!(formatter, "GDELT cache error: {error}"),
            Self::PacingPoisoned => formatter.write_str("GDELT request pacing lock poisoned"),
            Self::RateLimited(error) => write!(formatter, "GDELT rate limit: {error}"),
            Self::OfflineCacheMiss => formatter.write_str("offline cache miss for GDELT request"),
        }
    }
}

impl std::error::Error for Error {}

/// A date/value observation returned by a DOC timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    /// Date in `YYYYMMDD` form.
    pub date: String,
    /// Numeric timeline value.
    pub value: f64,
}

/// A configurable, blocking GDELT DOC client.
#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
    cache_dir: Option<PathBuf>,
    offline_cache_only: bool,
}

impl Client {
    /// Creates a client with no on-disk cache.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn new() -> Result<Self, Error> {
        Self::with_cache_dir(None)
    }

    /// Creates a client with an optional caller-owned cache directory.
    ///
    /// Cache entries are keyed by the complete request URL. Cached responses
    /// are returned without contacting GDELT.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn with_cache_dir(cache_dir: Option<&Path>) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!("statsapi-gdelt/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(40))
            .build()
            .map_err(Error::Http)?;
        Ok(Self {
            http,
            cache_dir: cache_dir.map(Path::to_path_buf),
            offline_cache_only: false,
        })
    }

    /// Returns a client that only reads cached responses and never accesses the network.
    #[must_use]
    pub fn offline_cache_only(mut self) -> Self {
        self.offline_cache_only = true;
        self
    }

    /// Fetches and decodes a timeline for a DOC mode and inclusive date range.
    ///
    /// `start` and `end` use `YYYYMMDD`; modes include `timelinevolraw` and
    /// `timelinetone`. Requests are paced to at least six seconds apart per
    /// process to respect GDELT's five-second guidance.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid URLs, failed HTTP requests, malformed
    /// JSON, missing timeline data, or cache I/O failures.
    pub fn timeline(
        &self,
        query: &str,
        mode: &str,
        start: &str,
        end: &str,
    ) -> Result<Vec<Point>, Error> {
        let url = Self::timeline_url(query, mode, start, end)?;
        self.fetch_timeline(&url)
    }

    /// Builds the DOC URL for a query, mode, and inclusive date range.
    ///
    /// # Errors
    ///
    /// Returns an error if the fixed endpoint URL cannot be parsed.
    pub fn timeline_url(query: &str, mode: &str, start: &str, end: &str) -> Result<String, Error> {
        let mut url = reqwest::Url::parse(API).map_err(|error| Error::Url(error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("query", query)
            .append_pair("mode", mode)
            .append_pair("format", "json")
            .append_pair("sort", "datedesc")
            .append_pair("startdatetime", &format!("{start}000000"))
            .append_pair("enddatetime", &format!("{end}235959"));
        Ok(url.into())
    }

    /// Fetches and decodes a previously built DOC timeline URL.
    ///
    /// # Errors
    ///
    /// Returns an error for failed HTTP requests, malformed JSON, missing
    /// timeline data, or cache I/O failures.
    pub fn fetch_timeline(&self, url: &str) -> Result<Vec<Point>, Error> {
        let body = self.fetch(url)?;
        let payload: Value = serde_json::from_slice(&body).map_err(Error::Json)?;
        decode_timeline(&payload)
    }

    fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        if let Some(path) = self.cache_path(url) {
            if let Ok(body) = std::fs::read(&path) {
                return Ok(body);
            }
        }
        if self.offline_cache_only {
            return Err(Error::OfflineCacheMiss);
        }
        let mut delay = jittered_backoff(0);
        let mut last = None;
        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 {
                thread::sleep(delay);
                delay = jittered_backoff(attempt);
            }
            let _request_slot = Self::request_slot()?;
            match self.http.get(url).send() {
                Ok(response) => {
                    let status = response.status();
                    let body = response.bytes().map_err(Error::Http)?.to_vec();
                    if retryable_status(status) || is_rate_limit_body(&body) {
                        let rate_limited =
                            status == StatusCode::TOO_MANY_REQUESTS || is_rate_limit_body(&body);
                        if attempt + 1 < MAX_ATTEMPTS {
                            last = Some(if rate_limited {
                                Error::RateLimited(response_preview(&body))
                            } else {
                                Error::Status(status)
                            });
                            continue;
                        }
                        if rate_limited {
                            return Err(Error::RateLimited(response_preview(&body)));
                        }
                        return Err(Error::Status(status));
                    }
                    if !status.is_success() {
                        return Err(Error::Status(status));
                    }
                    self.write_cache(url, &body)?;
                    return Ok(body);
                }
                Err(error) if attempt + 1 < MAX_ATTEMPTS => last = Some(Error::Http(error)),
                Err(error) => return Err(Error::Http(error)),
            }
        }
        Err(last.unwrap_or_else(|| Error::MissingTimeline))
    }

    fn request_slot() -> Result<MutexGuard<'static, Option<Instant>>, Error> {
        let mut last_request = LAST_REQUEST
            .get_or_init(|| Mutex::new(None))
            .lock()
            .map_err(|_| Error::PacingPoisoned)?;
        if let Some(last) = *last_request {
            if let Some(wait) = MIN_REQUEST_INTERVAL.checked_sub(last.elapsed()) {
                thread::sleep(wait);
            }
        }
        *last_request = Some(Instant::now());
        Ok(last_request)
    }

    fn cache_path(&self, url: &str) -> Option<PathBuf> {
        let dir = self.cache_dir.as_ref()?;
        let digest = Sha256::digest(url.as_bytes());
        Some(dir.join(format!("{digest:x}")))
    }

    fn write_cache(&self, url: &str, body: &[u8]) -> Result<(), Error> {
        let Some(path) = self.cache_path(url) else {
            return Ok(());
        };
        std::fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(".")))
            .map_err(Error::Cache)?;
        let temp = path.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&temp, body).map_err(Error::Cache)?;
        std::fs::rename(temp, path).map_err(Error::Cache)
    }
}

fn retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn is_rate_limit_body(body: &[u8]) -> bool {
    let body = String::from_utf8_lossy(body).to_ascii_lowercase();
    body.contains("please limit requests to one every 5 seconds")
        || body.contains("please limit requests to one every five seconds")
}

fn response_preview(body: &[u8]) -> String {
    String::from_utf8_lossy(body).chars().take(180).collect()
}

fn jittered_backoff(retry_index: u32) -> Duration {
    let base = Duration::from_secs(5_u64.saturating_mul(1_u64 << retry_index.min(3)));
    let upper_jitter_ms = if retry_index == 0 { 3_000 } else { 1_000 };
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    base + Duration::from_millis(u64::from(nanos) % (upper_jitter_ms + 1))
}

/// Decodes the standard DOC `timeline[].data[]` response layout.
///
/// # Errors
///
/// Returns [`Error::MissingTimeline`] when the upstream shape is absent.
pub fn decode_timeline(payload: &Value) -> Result<Vec<Point>, Error> {
    let points = payload
        .get("timeline")
        .and_then(Value::as_array)
        .and_then(|timeline| timeline.first())
        .and_then(|series| series.get("data"))
        .and_then(Value::as_array)
        .ok_or(Error::MissingTimeline)?;
    let mut decoded: Vec<Point> = points
        .iter()
        .filter_map(|point| {
            Some(Point {
                date: point.get("date")?.as_str()?.chars().take(8).collect(),
                value: point.get("value")?.as_f64()?,
            })
        })
        .collect();
    decoded.sort_by(|left, right| left.date.cmp(&right.date));
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_plain_text_rate_limit_and_starts_with_five_to_eight_seconds() {
        assert!(is_rate_limit_body(
            b"Please limit requests to one every 5 seconds or contact us."
        ));
        assert!(!is_rate_limit_body(b"{\"articles\": []}"));
        let first = jittered_backoff(0);
        assert!((Duration::from_secs(5)..=Duration::from_secs(8)).contains(&first));
    }
}
