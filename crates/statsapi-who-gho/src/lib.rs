//! Blocking client for the WHO Global Health Observatory `OData` API.

use reqwest::blocking::Client as HttpClient;
use reqwest::Url;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

/// A blocking WHO GHO client with optional caller-owned response caching.
#[derive(Clone, Debug)]
pub struct WhoGhoClient {
    http: HttpClient,
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
            cache_dir: None,
            offline: false,
            max_retries: 3,
        })
    }

    /// Stores responses under a caller-supplied directory.
    #[must_use]
    pub fn with_cache_dir(mut self, directory: impl AsRef<Path>) -> Self {
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
        let url = self.url(query, country)?;
        let cache_path = self
            .cache_dir
            .as_ref()
            .map(|directory| directory.join(cache_key(&url)));
        if let Some(path) = cache_path.as_ref() {
            if let Ok(body) = std::fs::read(path) {
                return Ok((url, serde_json::from_slice(&body)?));
            }
            if self.offline {
                return Err(ClientError::CacheMiss(path.clone()));
            }
        }
        let body = self.get_with_retry(&url)?;
        if let Some(path) = cache_path {
            write_cache(&path, &body)?;
        }
        Ok((url, serde_json::from_slice(&body)?))
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

fn cache_key(url: &str) -> String {
    Sha256::digest(url.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut digest, byte| {
            use std::fmt::Write as _;
            let _ = write!(digest, "{byte:02x}");
            digest
        })
}

fn write_cache(path: &Path, body: &[u8]) -> Result<(), ClientError> {
    std::fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(".")))?;
    let temp = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&temp, body)?;
    std::fs::rename(temp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
