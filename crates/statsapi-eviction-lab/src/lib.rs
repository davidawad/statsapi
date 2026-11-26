//! Blocking access and decoding for public Eviction Lab aggregate CSV files.

use ephemeral_cache::EphemeralCache;
pub use ephemeral_cache::Source;
use std::cell::Cell;
use std::fmt;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ATTEMPTS: u32 = 4;

/// Controls whether a request may use the network.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkPolicy {
    /// Read a cached response or fetch it from the upstream.
    Allow,
    /// Read only an existing cached response.
    CacheOnly,
}

/// Errors returned while downloading or decoding Eviction Lab files.
#[derive(Debug)]
pub enum Error {
    /// The requested response is not in the configured cache.
    CacheMiss,
    /// An HTTP request failed after retrying transient failures.
    Http(reqwest::Error),
    /// The upstream returned a non-success status.
    Status(reqwest::StatusCode),
    /// The upstream CSV could not be read or did not have expected columns.
    Csv(String),
    /// No non-empty observations matched the requested geography.
    NoRows(String),
    /// A CSV value that should be numeric was malformed.
    Number(String),
    /// The response could not be read or written in the cache.
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CacheMiss => f.write_str("offline cache miss"),
            Self::Http(error) => write!(f, "HTTP request failed: {error}"),
            Self::Status(status) => write!(f, "upstream returned HTTP {status}"),
            Self::Csv(message) => f.write_str(message),
            Self::NoRows(geography) => write!(f, "no ETS rows for geography {geography}"),
            Self::Number(value) => write!(f, "invalid numeric CSV value {value:?}"),
            Self::Io(error) => write!(f, "cache I/O failed: {error}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

/// Decoded aggregate for the most recent week available for a geography.
#[derive(Clone, Debug, PartialEq)]
pub struct WeeklyAggregate {
    /// Sum of current-period counts for the selected week.
    pub current: f64,
    /// Sum of the selected comparison column for the selected week.
    pub comparison: f64,
    /// ISO date identifying the selected week.
    pub week_date: String,
    /// Name of the CSV's second column, commonly a geography description.
    pub geography_column: String,
}

/// A blocking Eviction Lab client. It streams by default; a caller-owned cache
/// directory enables the last-resort TTL/size-capped ephemeral cache.
#[derive(Debug)]
pub struct Client {
    cache: Option<EphemeralCache>,
    source: Cell<Source>,
    http: reqwest::blocking::Client,
}

impl Client {
    /// Creates a client; a cache directory is optional and has no global default.
    ///
    /// # Errors
    ///
    /// Returns an HTTP client construction error when rustls or reqwest cannot initialize.
    pub fn new(cache_dir: Option<PathBuf>) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(90))
            .build()
            .map_err(Error::Http)?;
        Ok(Self {
            cache: cache_dir.map(EphemeralCache::bulk),
            source: Cell::new(Source::Stream),
            http,
        })
    }

    /// Downloads or loads a response, retrying transient network and server errors.
    ///
    /// # Errors
    ///
    /// Returns a cache miss under [`NetworkPolicy::CacheOnly`], a cache I/O failure,
    /// or the final HTTP error after retrying transient failures.
    pub fn fetch(&self, url: &str, policy: NetworkPolicy) -> Result<Vec<u8>, Error> {
        if let Some(bytes) = self.cache.as_ref().and_then(|cache| cache.get(url)) {
            self.source.set(Source::Cache);
            return Ok(bytes);
        }
        if policy == NetworkPolicy::CacheOnly {
            return Err(Error::CacheMiss);
        }

        let bytes = self.fetch_with_retries(url)?;
        self.source.set(Source::Stream);
        if let Some(cache) = &self.cache {
            cache.put(url, &bytes).map_err(Error::Io)?;
        }
        Ok(bytes)
    }

    /// Returns the path that answered the last [`Client::fetch`]: `stream`
    /// (download decoded in memory, nothing persisted) or `cache`.
    #[must_use]
    pub fn last_source(&self) -> Source {
        self.source.get()
    }

    /// Decodes the newest matching weekly ETS aggregate from its upstream CSV layout.
    ///
    /// # Errors
    ///
    /// Returns an error when the CSV is malformed, required columns are absent,
    /// no rows match the geography, or a numeric field is invalid.
    pub fn latest_week(
        &self,
        bytes: &[u8],
        comparison_column: &str,
        geography_prefix: &str,
    ) -> Result<WeeklyAggregate, Error> {
        latest_week(bytes, comparison_column, geography_prefix)
    }

    fn fetch_with_retries(&self, url: &str) -> Result<Vec<u8>, Error> {
        let mut last_error = None;
        for attempt in 0..ATTEMPTS {
            let response = self.http.get(url).send();
            match response {
                Ok(response) if response.status().is_success() => {
                    return response
                        .bytes()
                        .map(|body| body.to_vec())
                        .map_err(Error::Http);
                }
                Ok(response) => {
                    let status = response.status();
                    if !retryable_status(status) {
                        return Err(Error::Status(status));
                    }
                    last_error = Some(Error::Status(status));
                }
                Err(error) if error.is_timeout() || error.is_connect() || error.is_request() => {
                    last_error = Some(Error::Http(error));
                }
                Err(error) => return Err(Error::Http(error)),
            }
            if attempt + 1 < ATTEMPTS {
                thread::sleep(retry_delay(attempt));
            }
        }
        Err(last_error.unwrap_or(Error::CacheMiss))
    }
}

/// Decodes the latest matching week from the source CSV layout.
///
/// # Errors
///
/// Returns an error when the CSV is malformed, required columns are absent,
/// no rows match the geography, or a numeric field is invalid.
pub fn latest_week(
    bytes: &[u8],
    comparison_column: &str,
    geography_prefix: &str,
) -> Result<WeeklyAggregate, Error> {
    let mut reader = csv::Reader::from_reader(bytes);
    let headers = reader
        .headers()
        .map_err(|error| Error::Csv(format!("ETS CSV header: {error}")))?
        .clone();
    let index = |name: &str| {
        headers
            .iter()
            .position(|header| header == name)
            .ok_or_else(|| Error::Csv(format!("ETS CSV missing {name}")))
    };
    let geoid = index("GEOID")?;
    let date = index("week_date")?;
    let current = index("filings_2020")?;
    let comparison = index(comparison_column)?;
    let mut newest = String::new();
    for row in csv::Reader::from_reader(bytes).records() {
        let row = row.map_err(|error| Error::Csv(format!("ETS CSV row: {error}")))?;
        if row
            .get(geoid)
            .unwrap_or_default()
            .starts_with(geography_prefix)
            && !row.get(comparison).unwrap_or_default().is_empty()
        {
            let row_date = row.get(date).unwrap_or_default();
            if row_date > newest.as_str() {
                row_date.clone_into(&mut newest);
            }
        }
    }
    if newest.is_empty() {
        return Err(Error::NoRows(geography_prefix.to_owned()));
    }
    let mut current_total = 0.0;
    let mut comparison_total = 0.0;
    let mut matched = false;
    for row in csv::Reader::from_reader(bytes).records() {
        let row = row.map_err(|error| Error::Csv(format!("ETS CSV row: {error}")))?;
        if row
            .get(geoid)
            .unwrap_or_default()
            .starts_with(geography_prefix)
            && !row.get(comparison).unwrap_or_default().is_empty()
            && row.get(date).unwrap_or_default() == newest
        {
            current_total += parse_number(row.get(current).unwrap_or("0"))?;
            comparison_total += parse_number(row.get(comparison).unwrap_or("0"))?;
            matched = true;
        }
    }
    if !matched {
        return Err(Error::NoRows(geography_prefix.to_owned()));
    }
    Ok(WeeklyAggregate {
        current: current_total,
        comparison: comparison_total,
        week_date: newest,
        geography_column: headers.iter().nth(1).unwrap_or("geography").to_owned(),
    })
}

fn parse_number(value: &str) -> Result<f64, Error> {
    value.parse().map_err(|_| Error::Number(value.to_owned()))
}

fn retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn retry_delay(attempt: u32) -> Duration {
    let base_ms = 150_u64.saturating_mul(1_u64 << attempt);
    let jitter_ms = u64::from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_millis(),
    ) % (base_ms / 2 + 1);
    Duration::from_millis(base_ms + jitter_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_week_sums_synthetic_fixture_values() {
        let bytes = include_bytes!("../tests/fixtures/ets-weekly.csv");
        let selected = latest_week(bytes, "filings_avg_prepandemic_baseline", "12345")
            .expect("synthetic fixture should decode");
        assert_eq!(selected.current, 17.0, "latest current values should sum");
        assert_eq!(
            selected.comparison, 8.0,
            "latest comparison values should sum"
        );
        assert_eq!(
            selected.week_date, "2025-02-09",
            "latest week should be selected"
        );
    }

    #[test]
    fn cache_only_miss_is_explicit() {
        let client = Client::new(None).expect("client should initialize");
        assert!(
            matches!(
                client.fetch("https://invalid.example/data.csv", NetworkPolicy::CacheOnly),
                Err(Error::CacheMiss)
            ),
            "a cache-only request without a cache directory should report a miss"
        );
    }

    #[test]
    fn cached_entry_is_served_as_cache_and_expired_entry_is_not() {
        let dir = std::env::temp_dir().join(format!("eviction-lab-cache-{}", std::process::id()));
        let url = "https://example.invalid/ets.csv";
        ephemeral_cache::EphemeralCache::bulk(&dir)
            .put(url, b"a,b\n1,2\n")
            .expect("seed cache");
        let client = Client::new(Some(dir.clone())).expect("client");
        let bytes = client
            .fetch(url, NetworkPolicy::CacheOnly)
            .expect("fresh entry");
        assert_eq!(bytes, b"a,b\n1,2\n");
        assert_eq!(client.last_source(), Source::Cache);
        let expired = Client {
            cache: Some(EphemeralCache::new(&dir, Duration::ZERO, 1024)),
            ..Client::new(None).expect("client")
        };
        assert!(matches!(
            expired.fetch(url, NetworkPolicy::CacheOnly),
            Err(Error::CacheMiss)
        ));
        let _ = std::fs::remove_dir_all(dir);
    }
}
