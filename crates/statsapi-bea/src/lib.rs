//! Blocking client for the U.S. Bureau of Economic Analysis Regional API.
//!
//! The crate owns HTTP access, retry, response decoding, and optional caller-owned
//! disk caching. It has no dependency on application or repository crates.

use reqwest::StatusCode;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

/// Environment variable used by [`Client::from_env`].
pub const API_KEY_ENV: &str = "BEA_API_KEY";
const API_URL: &str = "https://apps.bea.gov/api/data/";

/// Errors returned while requesting or decoding BEA data.
#[derive(Debug, Error)]
pub enum Error {
    /// No API key was supplied or found in the environment.
    #[error("BEA API key is missing; pass it to Client::new or set {API_KEY_ENV}")]
    MissingKey,
    /// HTTP transport failed after retries.
    #[error("BEA request failed: {0}")]
    Transport(#[from] reqwest::Error),
    /// BEA returned an unsuccessful HTTP status after retries.
    #[error("BEA returned HTTP status {0}")]
    HttpStatus(StatusCode),
    /// Response JSON could not be decoded.
    #[error("BEA response JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    /// The BEA response has an error or lacks a matching observation.
    #[error("invalid BEA response: {0}")]
    Response(&'static str),
    /// A caller-supplied cache could not be read or written.
    #[error("BEA cache I/O failed: {0}")]
    Cache(#[from] std::io::Error),
}

/// Result type used by this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// One decoded Regional API observation.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    /// BEA numeric data value.
    pub value: f64,
    /// Observation period returned by BEA.
    pub year: String,
    /// Public API URL with the credential removed.
    pub source_url: String,
}

/// Blocking BEA Regional API client with caller-controlled cache and retry.
#[derive(Clone, Debug)]
pub struct Client {
    api_key: String,
    cache_dir: Option<PathBuf>,
    http: reqwest::blocking::Client,
    max_attempts: u32,
}

impl Client {
    /// Creates a client using the supplied API key and no disk cache.
    ///
    /// Use [`Client::with_cache_dir`] to enable an optional caller-supplied cache.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`] for an empty key or [`Error::Transport`] if
    /// the HTTP client cannot be initialized.
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(Error::MissingKey);
        }
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(90))
            .build()?;
        Ok(Self {
            api_key,
            cache_dir: None,
            http,
            max_attempts: 4,
        })
    }

    /// Creates a client using the key in [`API_KEY_ENV`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`] if the environment variable is absent or empty.
    pub fn from_env() -> Result<Self> {
        let key = std::env::var(API_KEY_ENV).map_err(|_| Error::MissingKey)?;
        Self::new(key)
    }

    /// Sets the directory used for content-addressed raw response caching.
    #[must_use]
    pub fn with_cache_dir(mut self, cache_dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(cache_dir.into());
        self
    }

    /// Requests and decodes the latest matching row from a Regional API table.
    ///
    /// `table`, `line_code`, `geo_fips`, and `year` map to BEA's `TableName`,
    /// `LineCode`, `GeoFips`, and `Year` request parameters. Cache files contain
    /// raw API responses and are named using a hash that excludes the API key.
    ///
    /// # Errors
    ///
    /// Returns an error for transport, HTTP, cache, decoding, or BEA response failures.
    pub fn get_data(
        &self,
        table: &str,
        line_code: &str,
        geo_fips: &str,
        year: &str,
    ) -> Result<Observation> {
        let mut url =
            reqwest::Url::parse(API_URL).map_err(|_| Error::Response("invalid API URL"))?;
        url.query_pairs_mut()
            .append_pair("UserID", &self.api_key)
            .append_pair("method", "GetData")
            .append_pair("datasetname", "Regional")
            .append_pair("TableName", table)
            .append_pair("LineCode", line_code)
            .append_pair("GeoFips", geo_fips)
            .append_pair("Year", year)
            .append_pair("ResultFormat", "JSON");
        let source_url = safe_url(&url);
        let raw = if let Some(path) = self.cache_path(&source_url) {
            match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let bytes = self.download(&url)?;
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(path, &bytes)?;
                    bytes
                }
                Err(error) => return Err(error.into()),
            }
        } else {
            self.download(&url)?
        };
        let (value, period) = decode_observation(&raw, line_code)?;
        Ok(Observation {
            value,
            year: period,
            source_url,
        })
    }

    fn cache_path(&self, source_url: &str) -> Option<PathBuf> {
        self.cache_dir.as_ref().map(|root| {
            let hash = format!("{:x}", Sha256::digest(source_url.as_bytes()));
            root.join(format!("{hash}.json"))
        })
    }

    fn download(&self, url: &reqwest::Url) -> Result<Vec<u8>> {
        for attempt in 0..self.max_attempts {
            match self.http.get(url.clone()).send() {
                Ok(response) if response.status().is_success() => match response.bytes() {
                    Ok(bytes) => return Ok(bytes.to_vec()),
                    Err(error) if attempt + 1 == self.max_attempts => {
                        return Err(Error::Transport(error));
                    }
                    Err(_) => {}
                },
                Ok(response) => {
                    let status = response.status();
                    if !retryable(status) || attempt + 1 == self.max_attempts {
                        return Err(Error::HttpStatus(status));
                    }
                }
                Err(error) => {
                    let retryable = error.is_timeout() || error.is_connect() || error.is_request();
                    if !retryable || attempt + 1 == self.max_attempts {
                        return Err(Error::Transport(error));
                    }
                }
            }
            let ceiling_ms = 100_u64.saturating_mul(1_u64 << attempt.min(7));
            std::thread::sleep(Duration::from_millis(fastrand::u64(0..=ceiling_ms)));
        }
        Err(Error::Response("retry loop ended unexpectedly"))
    }
}

/// Decodes a BEA Regional response and selects the latest matching line.
///
/// # Errors
///
/// Returns an error if the bytes are invalid JSON or contain no matching numeric row.
pub fn decode_observation(bytes: &[u8], line_code: &str) -> Result<(f64, String)> {
    let response: Value = serde_json::from_slice(bytes)?;
    if !response["BEAAPI"]["Results"]["Error"].is_null() {
        return Err(Error::Response(
            "BEA API returned an error (details redacted)",
        ));
    }
    let rows = response["BEAAPI"]["Results"]["Data"]
        .as_array()
        .ok_or(Error::Response("BEA response has no data"))?;
    let row = rows
        .iter()
        .filter(|row| row["LineCode"].as_str() == Some(line_code))
        .max_by_key(|row| {
            row["TimePeriod"]
                .as_str()
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or_default()
        })
        .ok_or(Error::Response("BEA returned no matching observations"))?;
    let value = row["DataValue"]
        .as_str()
        .ok_or(Error::Response("BEA response has no value"))?
        .replace(',', "")
        .parse::<f64>()
        .map_err(|_| Error::Response("BEA value is not numeric"))?;
    let year = row["TimePeriod"]
        .as_str()
        .ok_or(Error::Response("BEA response has no period"))?
        .to_owned();
    Ok((value, year))
}

fn safe_url(url: &reqwest::Url) -> String {
    let mut clean = url.clone();
    clean.query_pairs_mut().clear();
    for (key, value) in url.query_pairs() {
        if !key.eq_ignore_ascii_case("userid") {
            clean.query_pairs_mut().append_pair(&key, &value);
        }
    }
    clean.to_string()
}

fn retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_latest_matching_synthetic_fixture() {
        let bytes = include_bytes!("../tests/fixtures/regional-response.json");
        assert_eq!(
            decode_observation(bytes, "3").expect("fixture decodes"),
            (83_417.0, "2097".to_owned())
        );
    }

    #[test]
    fn credential_is_removed_from_source_url() {
        let url =
            reqwest::Url::parse("https://apps.bea.gov/api/data/?UserID=secret&method=GetData")
                .expect("URL parses");
        let safe = safe_url(&url);
        assert!(!safe.contains("secret"));
        assert!(safe.contains("method=GetData"));
    }

    #[test]
    #[ignore = "live BEA API check; requires BEA_API_KEY and network access"]
    fn live_regional_api_request() {
        let Ok(key) = std::env::var(API_KEY_ENV) else {
            eprintln!("credential_missing: {API_KEY_ENV}");
            return;
        };
        let client = Client::new(key).expect("client builds");
        let observation = client
            .get_data("CAINC1", "3", "00000", "LAST5")
            .expect("live BEA request");
        assert!(observation.value.is_finite());
    }
}
