//! Blocking client for the public NAEP Data Service API.

use reqwest::StatusCode;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ENDPOINT: &str = "https://www.nationsreportcard.gov/DataService/GetAdhocData.aspx";
const MAX_RETRIES: u32 = 3;

/// A request to the NAEP Data Service. Parameters follow the upstream API names.
#[derive(Debug, Clone, Default)]
pub struct Query {
    parameters: Vec<(String, String)>,
}

impl Query {
    /// Creates a data query with the required `type=data` parameter.
    #[must_use]
    pub fn new() -> Self {
        Self::default().parameter("type", "data")
    }

    /// Adds or replaces an upstream query parameter.
    #[must_use]
    pub fn parameter(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        let name = name.into();
        let value = value.into();
        if let Some(existing) = self.parameters.iter_mut().find(|(key, _)| key == &name) {
            existing.1 = value;
        } else {
            self.parameters.push((name, value));
        }
        self
    }

    /// Builds the fully encoded endpoint URL.
    ///
    /// # Panics
    ///
    /// Panics only if the hard-coded HTTPS endpoint is invalid.
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "the hard-coded upstream endpoint is valid"
    )]
    pub fn url(&self) -> String {
        let mut url = reqwest::Url::parse(ENDPOINT).expect("the built-in NAEP endpoint is valid");
        url.query_pairs_mut().extend_pairs(&self.parameters);
        url.into()
    }
}

/// The JSON response returned by the NAEP Data Service.
#[derive(Debug, Clone, Deserialize)]
pub struct Response {
    /// Upstream status value, normally 200.
    pub status: Option<u16>,
    /// Result rows in the upstream response layout.
    pub result: Vec<serde_json::Value>,
}

/// Errors returned by the client.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A transport or HTTP status failure.
    Http(reqwest::Error),
    /// The service returned an unsuccessful status that reqwest does not classify as an error.
    Status { url: String, status: u16 },
    /// An error reading or writing the optional response cache.
    Cache(std::io::Error),
    /// The response body was not valid NAEP JSON.
    Decode(serde_json::Error),
    /// Offline mode was requested and there was no cached response.
    CacheMiss(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(error) => write!(formatter, "NAEP HTTP request failed: {error}"),
            Self::Status { url, status } => {
                write!(formatter, "NAEP returned HTTP {status} for {url}")
            }
            Self::Cache(error) => write!(formatter, "NAEP response cache failed: {error}"),
            Self::Decode(error) => write!(formatter, "NAEP response JSON is invalid: {error}"),
            Self::CacheMiss(url) => write!(formatter, "offline cache miss for {url}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Cache(error) => Some(error),
            Self::Decode(error) => Some(error),
            Self::Status { .. } | Self::CacheMiss(_) => None,
        }
    }
}

/// A reusable NAEP API client with optional, caller-owned response caching.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Creates a client with no cache and online access enabled.
    #[must_use]
    pub fn new() -> Self {
        let http = reqwest::blocking::Client::new();
        Self {
            http,
            cache_dir: None,
            offline: false,
        }
    }

    /// Uses `directory` for a content-addressed cache of responses.
    #[must_use]
    pub fn with_cache_dir(mut self, directory: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(directory.as_ref().to_path_buf());
        self
    }

    /// Disables network access and requires each response to be cached.
    #[must_use]
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    /// Requests and decodes one NAEP Data Service response.
    ///
    /// # Errors
    ///
    /// Returns an error for HTTP failures, malformed JSON, cache I/O failures, or an offline
    /// cache miss.
    pub fn get(&self, query: &Query) -> Result<Response, Error> {
        let url = query.url();
        let bytes = if let Some(bytes) = self.read_cache(&url)? {
            bytes
        } else if self.offline {
            return Err(Error::CacheMiss(url));
        } else {
            let bytes = self.fetch(&url)?;
            self.write_cache(&url, &bytes)?;
            bytes
        };
        serde_json::from_slice(&bytes).map_err(Error::Decode)
    }

    /// Reads a response from the configured cache, if present.
    ///
    /// # Errors
    ///
    /// Returns an error if the cache cannot be read.
    pub fn cached(&self, query: &Query) -> Result<Option<Vec<u8>>, Error> {
        self.read_cache(&query.url())
    }

    fn read_cache(&self, url: &str) -> Result<Option<Vec<u8>>, Error> {
        let Some(path) = self.cache_path(url) else {
            return Ok(None);
        };
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Error::Cache(error)),
        }
    }

    fn write_cache(&self, url: &str, bytes: &[u8]) -> Result<(), Error> {
        let Some(path) = self.cache_path(url) else {
            return Ok(());
        };
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        std::fs::create_dir_all(parent).map_err(Error::Cache)?;
        let temp = path.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&temp, bytes).map_err(Error::Cache)?;
        std::fs::rename(temp, path).map_err(Error::Cache)
    }

    fn cache_path(&self, url: &str) -> Option<PathBuf> {
        self.cache_dir.as_ref().map(|directory| {
            let hash = Sha256::digest(url.as_bytes());
            directory.join(format!("{hash:x}"))
        })
    }

    fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        for attempt in 0..=MAX_RETRIES {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    return response
                        .bytes()
                        .map(|bytes| bytes.to_vec())
                        .map_err(Error::Http);
                }
                Ok(response) => {
                    let status = response.status();
                    if !retryable(status) || attempt == MAX_RETRIES {
                        return Err(Error::Status {
                            url: url.to_owned(),
                            status: status.as_u16(),
                        });
                    }
                    std::thread::sleep(retry_delay(attempt, response.headers().get("retry-after")));
                }
                Err(error) => {
                    if !error.is_timeout() && !error.is_connect() && attempt == MAX_RETRIES {
                        return Err(Error::Http(error));
                    }
                    if attempt == MAX_RETRIES {
                        return Err(Error::Http(error));
                    }
                    std::thread::sleep(retry_delay(attempt, None));
                }
            }
        }
        Err(Error::CacheMiss(url.to_owned()))
    }
}

fn retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn retry_delay(attempt: u32, retry_after: Option<&reqwest::header::HeaderValue>) -> Duration {
    if let Some(seconds) = retry_after
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        return Duration::from_secs(seconds.min(30));
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.subsec_nanos());
    let jitter_ms = u64::from(nanos % 251);
    Duration::from_millis(250_u64.saturating_mul(1_u64 << attempt) + jitter_ms)
}

#[cfg(test)]
mod tests {
    use super::{Client, Query};
    use sha2::{Digest, Sha256};

    #[test]
    fn query_encodes_parameters_and_replaces_existing_values() {
        let url = Query::new()
            .parameter("subject", "MAT")
            .parameter("stattype", "MN:MN")
            .parameter("subject", "RED")
            .url();
        assert!(url.contains("subject=RED"), "{url}");
        assert!(url.contains("stattype=MN%3AMN"), "{url}");
        assert_eq!(url.matches("subject=").count(), 1, "{url}");
    }

    #[test]
    fn offline_mode_reads_upstream_shaped_synthetic_fixture() {
        let temp = std::env::temp_dir().join(format!("statsapi-naep-{}", std::process::id()));
        let client = Client::new().with_cache_dir(&temp).offline(true);
        let query = Query::new()
            .parameter("subject", "MAT")
            .parameter("grade", "4")
            .parameter("subscale", "MRPCM")
            .parameter("variable", "TOTAL")
            .parameter("jurisdiction", "CA")
            .parameter("stattype", "MN:MN")
            .parameter("Year", "Current");
        std::fs::create_dir_all(&temp).expect("create fixture cache");
        let hash = Sha256::digest(query.url().as_bytes());
        std::fs::write(
            temp.join(format!("{hash:x}")),
            include_bytes!("../tests/fixtures/data-response.json"),
        )
        .expect("write fixture cache");
        let response = client.get(&query).expect("fixture parses");
        assert_eq!(response.status, Some(200));
        assert_eq!(response.result[0]["value"], 217.5);
        let _ = std::fs::remove_dir_all(temp);
    }
}
