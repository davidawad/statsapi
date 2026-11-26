//! Blocking client for the WHO Global Health Observatory `OData` API.

use ephemeral_cache::EphemeralCache;
pub use ephemeral_cache::Source;
use reqwest::blocking::Client as HttpClient;
use reqwest::Url;
use serde_json::Value;
use std::fmt;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Cached responses older than this are never served (live responses: hours).
pub const CACHE_TTL: Duration = Duration::from_hours(6);
/// The cache directory is pruned oldest-first to at most this many bytes.
pub const CACHE_CAP_BYTES: u64 = 8 * 1024 * 1024;
const BASE: &str = "https://ghoapi.azureedge.net/api";
const DIMENSIONS: [&str; 6] = ["Dim1", "Dim2", "Dim3", "Dim4", "Dim5", "Dim6"];

/// A WHO GHO indicator request with optional `OData` dimensions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndicatorQuery {
    indicator: String,
    dimensions: Vec<(String, String)>,
}

impl IndicatorQuery {
    /// Creates a query for an `OData` indicator code.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::InvalidQuery`] when the code is empty or contains
    /// characters other than ASCII letters, digits, and underscores.
    pub fn new(indicator: impl Into<String>) -> Result<Self, ClientError> {
        let indicator = indicator.into();
        if indicator.is_empty()
            || !indicator
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(ClientError::InvalidQuery("invalid indicator code".into()));
        }
        Ok(Self {
            indicator,
            dimensions: Vec::new(),
        })
    }

    /// Adds a dimension such as `Dim1=SEX_BTSX` to the `OData` filter.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::InvalidQuery`] for unknown dimensions or empty
    /// values.
    pub fn with_dimension(
        mut self,
        dimension: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, ClientError> {
        let dimension = dimension.into();
        let value = value.into();
        if !DIMENSIONS.contains(&dimension.as_str()) || value.is_empty() {
            return Err(ClientError::InvalidQuery("invalid dimension".into()));
        }
        self.dimensions.push((dimension, value));
        Ok(self)
    }

    /// Returns the `OData` indicator code.
    #[must_use]
    pub fn indicator(&self) -> &str {
        &self.indicator
    }

    /// Returns the number of dimension filters in this query.
    #[must_use]
    pub fn dimension_count(&self) -> usize {
        self.dimensions.len()
    }

    fn filter(&self, country: &str) -> String {
        let mut filter = format!("SpatialDim eq '{country}'");
        for (dimension, value) in &self.dimensions {
            let value = value.replace('\'', "''");
            let _ = write!(filter, " and {dimension} eq '{value}'");
        }
        filter
    }
}

/// Errors returned while building or fetching a WHO GHO response.
#[derive(Debug)]
#[non_exhaustive]
pub enum ClientError {
    /// A query or country code is malformed.
    InvalidQuery(String),
    /// An HTTP request failed after retries.
    Http(reqwest::Error),
    /// The response body was not valid JSON.
    Json(serde_json::Error),
    /// Offline mode could not find a cached response.
    CacheMiss(PathBuf),
    /// A cache operation failed.
    Cache(std::io::Error),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQuery(message) => write!(f, "invalid query: {message}"),
            Self::Http(error) => write!(f, "WHO GHO request failed: {error}"),
            Self::Json(error) => write!(f, "invalid WHO GHO JSON: {error}"),
            Self::CacheMiss(path) => write!(f, "offline cache miss: {}", path.display()),
            Self::Cache(error) => write!(f, "WHO GHO cache failed: {error}"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Cache(error) => Some(error),
            Self::InvalidQuery(_) | Self::CacheMiss(_) => None,
        }
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<std::io::Error> for ClientError {
    fn from(error: std::io::Error) -> Self {
        Self::Cache(error)
    }
}

/// A blocking WHO GHO client: live API first, with an optional ephemeral
/// response cache (see [`CACHE_TTL`], [`CACHE_CAP_BYTES`]) as the last resort.
#[derive(Clone, Debug)]
pub struct WhoGhoClient {
    http: HttpClient,
    cache: Option<EphemeralCache>,
    cache_dir: Option<PathBuf>,
    offline: bool,
    max_retries: usize,
}

impl WhoGhoClient {
    /// Creates a client with network access, no cache, and three retries.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be initialized.
    pub fn new() -> Result<Self, ClientError> {
        Ok(Self {
            http: HttpClient::builder()
                .user_agent(concat!("statsapi-who-gho/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(30))
                .build()?,
            cache: None,
            cache_dir: None,
            offline: false,
            max_retries: 3,
        })
    }

    /// Keeps a last-resort ephemeral cache under a caller-supplied directory.
    #[must_use]
    pub fn with_cache_dir(mut self, directory: impl AsRef<Path>) -> Self {
        self.cache = Some(EphemeralCache::new(
            directory.as_ref(),
            CACHE_TTL,
            CACHE_CAP_BYTES,
        ));
        self.cache_dir = Some(directory.as_ref().to_path_buf());
        self
    }

    /// Makes requests read only from the configured cache.
    #[must_use]
    pub fn offline(mut self) -> Self {
        self.offline = true;
        self
    }

    /// Sets the number of retries after the initial attempt.
    #[must_use]
    pub fn with_max_retries(mut self, retries: usize) -> Self {
        self.max_retries = retries;
        self
    }

    /// Builds the upstream URL for inspection or attribution.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::InvalidQuery`] if the country is not an
    /// uppercase ISO alpha-3 code.
    pub fn url(&self, query: &IndicatorQuery, country: &str) -> Result<String, ClientError> {
        validate_country(country)?;
        let mut url = Url::parse(&format!("{BASE}/{}", query.indicator))
            .map_err(|error| ClientError::InvalidQuery(error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("$filter", &query.filter(country))
            .append_pair("$orderby", "TimeDim desc")
            .append_pair("$top", "1000");
        Ok(url.into())
    }

    /// Fetches and decodes an indicator's `OData` JSON document.
    ///
    /// Returns the exact URL alongside the decoded document.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid country codes, network failures, HTTP
    /// errors, invalid JSON, or cache failures.
    pub fn fetch(
        &self,
        query: &IndicatorQuery,
        country: &str,
    ) -> Result<(String, Value), ClientError> {
        self.fetch_sourced(query, country)
            .map(|(url, value, _)| (url, value))
    }

    /// Like [`Self::fetch`], also reporting the access path used: the live API
    /// answers first and the ephemeral cache only when it fails.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid country codes, network failures with no
    /// usable cache entry, HTTP errors, invalid JSON, or cache failures.
    pub fn fetch_sourced(
        &self,
        query: &IndicatorQuery,
        country: &str,
    ) -> Result<(String, Value, Source), ClientError> {
        let url = self.url(query, country)?;
        let cached = || self.cache.as_ref().and_then(|cache| cache.get(&url));
        let (body, source) = if self.offline {
            let body = cached().ok_or_else(|| {
                ClientError::CacheMiss(self.cache_dir.clone().unwrap_or_default())
            })?;
            (body, Source::Cache)
        } else {
            match self.get_with_retry(&url) {
                Ok(body) => {
                    if let Some(cache) = &self.cache {
                        cache.put(&url, &body)?;
                    }
                    (body, Source::Api)
                }
                Err(error) => match cached() {
                    Some(body) => (body, Source::Cache),
                    None => return Err(error),
                },
            }
        };
        let value = serde_json::from_slice(&body)?;
        Ok((url, value, source))
    }

    fn get_with_retry(&self, url: &str) -> Result<Vec<u8>, ClientError> {
        for attempt in 0..=self.max_retries {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    return Ok(response.bytes()?.to_vec());
                }
                Ok(response) if !retryable_status(response.status().as_u16()) => {
                    return match response.error_for_status() {
                        Err(error) => Err(ClientError::Http(error)),
                        Ok(_) => unreachable!("non-success status checked before conversion"),
                    };
                }
                Ok(_) | Err(_) if attempt < self.max_retries => jittered_delay(attempt),
                Ok(response) => {
                    return match response.error_for_status() {
                        Err(error) => Err(ClientError::Http(error)),
                        Ok(_) => unreachable!("non-success status checked before conversion"),
                    };
                }
                Err(error) => return Err(ClientError::Http(error)),
            }
        }
        unreachable!("request loop returns on success or final failure")
    }
}

/// Selects the newest non-null numeric row, preferring both-sex and total rows.
#[must_use]
pub fn latest_observation(payload: &Value) -> Option<&Value> {
    let rows = payload.get("value")?.as_array()?;
    let newest = rows
        .iter()
        .filter(|row| {
            row.get("NumericValue")
                .is_some_and(|value| !value.is_null())
        })
        .filter_map(|row| {
            row.get("TimeDim")
                .and_then(Value::as_i64)
                .map(|year| (year, row))
        })
        .map(|(year, _)| year)
        .max()?;
    rows.iter()
        .filter(|row| {
            row.get("TimeDim").and_then(Value::as_i64) == Some(newest)
                && row
                    .get("NumericValue")
                    .is_some_and(|value| !value.is_null())
        })
        .max_by_key(|row| dimension_score(row))
}

fn dimension_score(row: &Value) -> (usize, usize, std::cmp::Reverse<String>) {
    let mut preferred = 0;
    let mut sex = 0;
    let mut tie_breaker = String::new();
    for dimension in DIMENSIONS {
        let Some(value) = row.get(dimension).and_then(Value::as_str) else {
            continue;
        };
        if value == "SEX_BTSX" {
            sex = 1;
            preferred += 1;
        } else if value.ends_with("_TOTL") {
            preferred += 1;
        }
        tie_breaker.push_str(value);
        tie_breaker.push('\0');
    }
    (preferred, sex, std::cmp::Reverse(tie_breaker))
}

fn validate_country(country: &str) -> Result<(), ClientError> {
    if country.len() != 3 || !country.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(ClientError::InvalidQuery(
            "country must be ISO alpha-3".into(),
        ));
    }
    Ok(())
}

fn retryable_status(status: u16) -> bool {
    status == 429 || (500..=599).contains(&status)
}

fn jittered_delay(attempt: usize) {
    let base_ms = 200_u64.saturating_mul(1_u64 << attempt.min(5));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let delay = base_ms / 2 + u64::from(nanos) % base_ms;
    std::thread::sleep(Duration::from_millis(delay));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded(name: &str, age: Duration) -> (WhoGhoClient, IndicatorQuery) {
        let dir = std::env::temp_dir().join(format!("who-cache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let query = IndicatorQuery::new("SDGSUICIDE").expect("query");
        let client = WhoGhoClient::new()
            .expect("client")
            .with_cache_dir(&dir)
            .offline();
        let url = client.url(&query, "USA").expect("url");
        EphemeralCache::new(&dir, CACHE_TTL, CACHE_CAP_BYTES)
            .put(&url, br#"{"value":[]}"#)
            .expect("seed");
        for entry in std::fs::read_dir(&dir).expect("dir").flatten() {
            let file = std::fs::File::options()
                .write(true)
                .open(entry.path())
                .expect("open");
            file.set_modified(std::time::SystemTime::now() - age)
                .expect("mtime");
        }
        (client, query)
    }

    #[test]
    fn offline_fresh_entry_is_labeled_cache() {
        let (client, query) = seeded("fresh", Duration::from_secs(60));
        let (_, _, source) = client.fetch_sourced(&query, "USA").expect("cached");
        assert_eq!(source, Source::Cache);
    }

    #[test]
    fn offline_expired_entry_is_a_miss() {
        let (client, query) = seeded("expired", CACHE_TTL + Duration::from_secs(60));
        assert!(matches!(
            client.fetch_sourced(&query, "USA"),
            Err(ClientError::CacheMiss(_))
        ));
    }

    #[test]
    fn latest_row_prefers_sex_and_total_for_the_newest_year() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/suicide-usa.json"))
                .expect("fixture JSON");
        let row = latest_observation(&fixture).expect("fixture observation");
        assert_eq!(row["TimeDim"], 2023);
        assert_eq!(row["Dim1"], "SEX_BTSX");
        assert_eq!(row["NumericValue"], 17.1);
    }

    #[test]
    fn query_escapes_dimensions_and_rejects_bad_country() {
        let query = IndicatorQuery::new("MH_12")
            .expect("valid code")
            .with_dimension("Dim1", "SEX_BTSX")
            .expect("valid dimension");
        let url = WhoGhoClient::new()
            .expect("HTTP client builds")
            .url(&query, "USA")
            .expect("valid country");
        let parsed = Url::parse(&url).expect("client URL");
        assert!(parsed.query_pairs().any(|(key, value)| {
            key == "$filter" && value == "SpatialDim eq 'USA' and Dim1 eq 'SEX_BTSX'"
        }));
        assert!(WhoGhoClient::new()
            .expect("HTTP client builds")
            .url(&query, "us")
            .is_err());
    }

    #[test]
    fn rejects_invalid_dimension_and_code() {
        assert!(IndicatorQuery::new("MH 12").is_err());
        assert!(IndicatorQuery::new("MH_12")
            .expect("valid code")
            .with_dimension("Dim7", "x")
            .is_err());
    }

    #[test]
    #[ignore = "live WHO GHO OData smoke test"]
    fn live_fetch_returns_an_observation() {
        let query = IndicatorQuery::new("MH_12")
            .expect("valid code")
            .with_dimension("Dim1", "SEX_BTSX")
            .expect("valid dimension");
        let (_, payload) = WhoGhoClient::new()
            .expect("HTTP client builds")
            .fetch(&query, "USA")
            .expect("WHO response");
        assert!(latest_observation(&payload).is_some());
    }
}
