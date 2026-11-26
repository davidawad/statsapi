//! Blocking client for World Bank country metadata and indicator observations.

use ephemeral_cache::EphemeralCache;
pub use ephemeral_cache::Source;
use reqwest::StatusCode;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

const BASE: &str = "https://api.worldbank.org/v2";
const MAX_ATTEMPTS: u32 = 4;
/// Cached responses older than this are never served (live responses: hours).
pub const CACHE_TTL: Duration = Duration::from_hours(6);
/// The cache directory is pruned oldest-first to at most this many bytes.
pub const CACHE_CAP_BYTES: u64 = 8 * 1024 * 1024;

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
    cache: Option<EphemeralCache>,
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
            cache: cache_dir.map(|dir| EphemeralCache::new(dir, CACHE_TTL, CACHE_CAP_BYTES)),
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
        self.country_region_sourced(country)
            .map(|(region, _)| region)
    }

    /// Like [`Self::country_region`], also reporting the access path used.
    ///
    /// # Errors
    /// Returns an error for transport, response, or missing region failures.
    pub fn country_region_sourced(&self, country: &str) -> Result<(Region, Source), Error> {
        let url = format!("{BASE}/country/{country}?format=json");
        let (response, source) = self.get_json(&url)?;
        Ok((parse_region(&response)?, source))
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
        self.indicator_sourced(geography, indicator, source)
            .map(|(value, _)| value)
    }

    /// Like [`Self::indicator`], also reporting the access path used.
    ///
    /// # Errors
    /// Returns an error for transport or invalid JSON responses.
    pub fn indicator_sourced(
        &self,
        geography: &str,
        indicator: &str,
        source: Option<&str>,
    ) -> Result<(Value, Source), Error> {
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

    fn get_json(&self, url: &str) -> Result<(Value, Source), Error> {
        let (bytes, source) = self.fetch(url)?;
        Ok((serde_json::from_slice(&bytes).map_err(Error::Json)?, source))
    }

    /// Live API first; the ephemeral cache answers only when the API fails
    /// (or the client is offline).
    fn fetch(&self, url: &str) -> Result<(Vec<u8>, Source), Error> {
        let cached = || self.cache.as_ref().and_then(|cache| cache.get(url));
        if self.offline {
            return cached()
                .map(|body| (body, Source::Cache))
                .ok_or_else(|| Error::OfflineCacheMiss(url.to_string()));
        }
        match self.request_with_retry(url) {
            Ok(body) => {
                if let Some(cache) = &self.cache {
                    cache.put(url, &body).map_err(Error::Io)?;
                }
                Ok((body, Source::Api))
            }
            Err(error) => cached().map(|body| (body, Source::Cache)).ok_or(error),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_client(name: &str, age: Duration) -> (Client, String) {
        let dir = std::env::temp_dir().join(format!("wb-cache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let url = Client::indicator_url("USA", "SP.POP.TOTL", None);
        EphemeralCache::new(&dir, CACHE_TTL, CACHE_CAP_BYTES)
            .put(&url, br#"[{},[{"value":1,"date":"2024"}]]"#)
            .expect("seed");
        for entry in std::fs::read_dir(&dir).expect("dir").flatten() {
            let file = std::fs::File::options()
                .write(true)
                .open(entry.path())
                .expect("open");
            file.set_modified(std::time::SystemTime::now() - age)
                .expect("mtime");
        }
        (
            Client::new(Some(dir)).expect("client").with_offline(true),
            url,
        )
    }

    #[test]
    fn offline_fresh_entry_is_labeled_cache() {
        let (client, _) = seeded_client("fresh", Duration::from_secs(60));
        let (payload, source) = client
            .indicator_sourced("USA", "SP.POP.TOTL", None)
            .expect("cached");
        assert_eq!(source, Source::Cache);
        assert_eq!(
            Client::latest_observation(&payload).expect("row")["value"],
            1
        );
    }

    #[test]
    fn offline_expired_entry_is_a_miss() {
        let (client, _) = seeded_client("expired", CACHE_TTL + Duration::from_secs(60));
        assert!(matches!(
            client.indicator_sourced("USA", "SP.POP.TOTL", None),
            Err(Error::OfflineCacheMiss(_))
        ));
    }

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
