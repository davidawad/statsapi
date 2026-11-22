//! Blocking client for World Bank country metadata and indicator observations.

use reqwest::StatusCode;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Duration;

const BASE: &str = "https://api.worldbank.org/v2";
const MAX_ATTEMPTS: u32 = 4;

/// Errors returned while requesting or decoding World Bank API responses.
#[derive(Debug)]
pub enum Error {
    /// The HTTP client could not be configured or a request failed.
    Http(reqwest::Error),
    /// The response body was not valid JSON.
    Json(serde_json::Error),
    /// A successful JSON response did not contain the requested metadata.
    MissingField(&'static str),
    /// Local cache I/O failed.
    Io(std::io::Error),
    /// The requested URL was not present in an offline cache.
    OfflineCacheMiss(String),
    /// The server rejected a request with a non-retryable status.
    HttpStatus(StatusCode),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(error) => write!(formatter, "world bank request failed: {error}"),
            Self::Json(error) => write!(formatter, "invalid world bank JSON: {error}"),
            Self::MissingField(field) => write!(formatter, "world bank response missing {field}"),
            Self::Io(error) => write!(formatter, "world bank cache I/O failed: {error}"),
            Self::OfflineCacheMiss(url) => write!(formatter, "offline cache miss for {url}"),
            Self::HttpStatus(status) => write!(formatter, "world bank returned HTTP {status}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::MissingField(_) | Self::OfflineCacheMiss(_) | Self::HttpStatus(_) => None,
        }
    }
}

/// World Bank country region identifier and display label.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Region {
    /// World Bank region code.
    pub id: String,
    /// Human-readable region label.
    pub name: String,
}

/// A blocking World Bank API client with optional caller-owned disk caching.
#[derive(Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl Client {
    /// Creates a client; cached responses are stored in the optional directory.
    ///
    /// # Errors
    /// Returns an error if the underlying HTTP client cannot be built.
    pub fn new(cache_dir: Option<PathBuf>) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!("statsapi-worldbank/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(Error::Http)?;
        Ok(Self {
            http,
            cache_dir,
            offline: false,
        })
    }

    /// Sets whether cache misses should fail without network access.
    #[must_use]
    pub const fn with_offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    /// Gets a country's World Bank region identifier and label.
    ///
    /// # Errors
    /// Returns an error for transport, response, or missing region failures.
    pub fn country_region(&self, country: &str) -> Result<Region, Error> {
        let url = format!("{BASE}/country/{country}?format=json");
        let response = self.get_json(&url)?;
        parse_region(&response)
    }

    /// Builds the endpoint URL used for an indicator request.
    #[must_use]
    pub fn indicator_url(geography: &str, indicator: &str, source: Option<&str>) -> String {
        let mut url =
            format!("{BASE}/country/{geography}/indicator/{indicator}?format=json&mrnev=1");
        if let Some(source) = source {
            url.push_str("&source=");
            url.push_str(source);
        }
        url
    }

    /// Gets an indicator response for a country or World Bank region code.
    ///
    /// `source` selects an optional World Bank database identifier.
    ///
    /// # Errors
    /// Returns an error for transport or invalid JSON responses.
    pub fn indicator(
        &self,
        geography: &str,
        indicator: &str,
        source: Option<&str>,
    ) -> Result<Value, Error> {
        let url = Self::indicator_url(geography, indicator, source);
        self.get_json(&url)
    }

    /// Returns the newest non-null observation row in an API response.
    #[must_use]
    pub fn latest_observation(payload: &Value) -> Option<&Value> {
        payload
            .get(1)?
            .as_array()?
            .iter()
            .find(|row| !row.get("value").is_none_or(Value::is_null))
    }

    fn get_json(&self, url: &str) -> Result<Value, Error> {
        let bytes = self.fetch(url)?;
        serde_json::from_slice(&bytes).map_err(Error::Json)
    }

    fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        if let Some(cache_dir) = &self.cache_dir {
            let path = cache_dir.join(cache_key(url));
            match std::fs::read(&path) {
                Ok(body) => return Ok(body),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(Error::Io(error)),
            }
            if self.offline {
                return Err(Error::OfflineCacheMiss(url.to_string()));
            }
            let body = self.request_with_retry(url)?;
            std::fs::create_dir_all(cache_dir).map_err(Error::Io)?;
            let temp = path.with_extension(format!("tmp.{}", std::process::id()));
            std::fs::write(&temp, &body).map_err(Error::Io)?;
            std::fs::rename(&temp, &path).map_err(Error::Io)?;
            return Ok(body);
        }
        if self.offline {
            return Err(Error::OfflineCacheMiss(url.to_string()));
        }
        self.request_with_retry(url)
    }

    fn request_with_retry(&self, url: &str) -> Result<Vec<u8>, Error> {
        for attempt in 0..MAX_ATTEMPTS {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    return response
                        .bytes()
                        .map(|bytes| bytes.to_vec())
                        .map_err(Error::Http);
                }
                Ok(response) => {
                    let status = response.status();
                    if !is_retryable(status) || attempt + 1 == MAX_ATTEMPTS {
                        return Err(Error::HttpStatus(status));
                    }
                }
                Err(error) if attempt + 1 == MAX_ATTEMPTS => return Err(Error::Http(error)),
                Err(_) => {}
            }
            let jitter_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| u64::from(elapsed.subsec_nanos() % 97));
            std::thread::sleep(Duration::from_millis(100 * (1 << attempt) + jitter_ms));
        }
        unreachable!("retry loop returns on its final attempt")
    }
}

fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn parse_region(response: &Value) -> Result<Region, Error> {
    let record = response
        .get(1)
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .ok_or(Error::MissingField("country metadata"))?;
    let id = record
        .pointer("/region/id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && *id != "NA")
        .ok_or(Error::MissingField("region id"))?;
    let name = record
        .pointer("/region/value")
        .and_then(Value::as_str)
        .unwrap_or(id);
    Ok(Region {
        id: id.to_string(),
        name: name.to_string(),
    })
}

fn cache_key(url: &str) -> String {
    Sha256::digest(url.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut digest, byte| {
            let _ = std::fmt::Write::write_fmt(&mut digest, format_args!("{byte:02x}"));
            digest
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_country_region_from_synthetic_upstream_layout() {
        let payload: Value = serde_json::from_str(include_str!("../tests/fixtures/country.json"))
            .expect("synthetic fixture is valid JSON");
        assert_eq!(
            parse_region(&payload).expect("region parses"),
            Region {
                id: "SYN".to_string(),
                name: "Synthetic Region".to_string(),
            }
        );
    }

    #[test]
    fn selects_latest_non_null_from_synthetic_indicator_layout() {
        let payload: Value = serde_json::from_str(include_str!("../tests/fixtures/indicator.json"))
            .expect("synthetic fixture is valid JSON");
        let row = Client::latest_observation(&payload).expect("fixture has a value");
        assert_eq!(row["date"], "2098");
        assert_eq!(row["value"], 12345);
    }
}
