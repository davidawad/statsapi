//! A reusable blocking client for building Socrata requests and decoding dataset rows.

use reqwest::blocking::Client as HttpClient;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use url::Url;

const MAX_ATTEMPTS: u32 = 4;

/// Errors produced while constructing or executing a Socrata request.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// The host or endpoint URL was invalid.
    #[error("invalid Socrata URL")]
    InvalidUrl,
    /// The HTTP client could not be built or the request failed.
    #[error("Socrata HTTP request failed")]
    Transport,
    /// The optional response cache could not be read or written.
    #[error("Socrata cache I/O failed")]
    CacheIo,
    /// The response body was not valid JSON.
    #[error("invalid Socrata JSON response")]
    InvalidJson,
    /// The response was JSON but not an array.
    #[error("Socrata response must be a JSON array")]
    InvalidResponse,
    /// The server returned a non-success status that should not be retried.
    #[error("Socrata returned HTTP status {0}")]
    HttpStatus(u16),
    /// The retry loop ended without recording a request error.
    #[error("Socrata retry loop ended without an error")]
    RetryExhausted,
    /// Offline operation was requested but no cached response exists.
    #[error("offline cache miss for {0}")]
    CacheMiss(String),
}

/// A blocking Socrata client with caller-owned authentication and optional cache storage.
#[derive(Debug, Clone)]
pub struct SocrataClient {
    http: HttpClient,
    app_token: Option<String>,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl SocrataClient {
    /// Creates an unauthenticated client without a cache.
    ///
    /// # Errors
    /// Returns an error if the underlying HTTP client cannot be configured.
    pub fn new() -> Result<Self, Error> {
        Self::builder().build()
    }

    /// Creates a client builder.
    #[must_use]
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Builds the resource URL for a dataset and its `SoQL` query parameters.
    ///
    /// Parameter names may be supplied with or without the `$` prefix. Values are URL encoded.
    ///
    /// # Errors
    /// Returns an error if the domain or constructed URL is invalid.
    pub fn resource_url(
        domain: &str,
        dataset: &str,
        parameters: &[(String, String)],
    ) -> Result<String, Error> {
        let mut url = Url::parse(&format!("https://{domain}/resource/{dataset}.json"))
            .map_err(|_| Error::InvalidUrl)?;
        {
            let mut query = url.query_pairs_mut();
            for (key, value) in parameters {
                let key = key.strip_prefix('$').unwrap_or(key);
                query.append_pair(&format!("${key}"), value);
            }
        }
        Ok(url.to_string())
    }

    /// Fetches and decodes all rows from a Socrata JSON resource URL.
    ///
    /// Responses are cached by URL when a cache directory was supplied at construction.
    ///
    /// # Errors
    /// Returns an error for network, cache, JSON, or empty-response failures.
    pub fn get_rows(&self, url: &str) -> Result<Vec<Value>, Error> {
        let bytes = self.fetch(url)?;
        decode_rows(&bytes)
    }

    fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        let cache_path = self.cache_dir.as_deref().map(|directory| {
            let digest = Sha256::digest(url.as_bytes());
            let name = digest
                .iter()
                .fold(String::with_capacity(64), |mut output, byte| {
                    let _ = std::fmt::Write::write_fmt(&mut output, format_args!("{byte:02x}"));
                    output
                });
            directory.join(name)
        });
        if let Some(path) = &cache_path {
            if let Ok(bytes) = std::fs::read(path) {
                return Ok(bytes);
            }
        }
        if self.offline {
            return Err(Error::CacheMiss(url.to_owned()));
        }
        let mut last_error = None;
        for attempt in 0..MAX_ATTEMPTS {
            let mut request = self.http.get(url);
            if let Some(token) = self.app_token.as_deref() {
                request = request.header("X-App-Token", token);
            }
            match request.send() {
                Ok(response) if response.status().is_success() => {
                    let bytes = response.bytes().map_err(|_| Error::Transport)?.to_vec();
                    if let Some(path) = &cache_path {
                        std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))
                            .map_err(|_| Error::CacheIo)?;
                        let temp = path.with_extension(format!("tmp.{}", std::process::id()));
                        std::fs::write(&temp, &bytes).map_err(|_| Error::CacheIo)?;
                        std::fs::rename(&temp, path).map_err(|_| Error::CacheIo)?;
                    }
                    return Ok(bytes);
                }
                Ok(response) => {
                    let status = response.status();
                    let error = Error::HttpStatus(status.as_u16());
                    if status.as_u16() != 429 && !status.is_server_error() {
                        return Err(error);
                    }
                    last_error = Some(error);
                }
                Err(_) => last_error = Some(Error::Transport),
            }
            if attempt + 1 < MAX_ATTEMPTS {
                let exponential_ms = 200_u64.saturating_mul(1_u64 << attempt).min(2_000);
                let jitter_ms = jitter(exponential_ms / 2);
                thread::sleep(Duration::from_millis(exponential_ms / 2 + jitter_ms));
            }
        }
        Err(last_error.unwrap_or(Error::RetryExhausted))
    }
}

/// Options for constructing a [`SocrataClient`].
#[derive(Debug, Default)]
pub struct ClientBuilder {
    app_token: Option<String>,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl ClientBuilder {
    /// Sets the optional Socrata application token.
    #[must_use]
    pub fn app_token(mut self, token: impl Into<String>) -> Self {
        self.app_token = Some(token.into());
        self
    }

    /// Sets an optional caller-owned response cache directory.
    #[must_use]
    pub fn cache_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(path.into());
        self
    }

    /// Uses only cached responses and reports cache misses instead of accessing the network.
    #[must_use]
    pub fn offline(mut self) -> Self {
        self.offline = true;
        self
    }

    /// Builds the client.
    ///
    /// # Errors
    /// Returns an error if the underlying HTTP client cannot be configured.
    pub fn build(self) -> Result<SocrataClient, Error> {
        let http = HttpClient::builder()
            .user_agent("statsapi-socrata/0.1.0")
            .timeout(Duration::from_secs(90))
            .build()
            .map_err(|_| Error::Transport)?;
        Ok(SocrataClient {
            http,
            app_token: self.app_token,
            cache_dir: self.cache_dir,
            offline: self.offline,
        })
    }
}

/// Decodes a Socrata response body as an array of JSON records.
///
/// # Errors
/// Returns an error for invalid JSON or a top-level value that is not an array.
pub fn decode_rows(bytes: &[u8]) -> Result<Vec<Value>, Error> {
    let rows: Value = serde_json::from_slice(bytes).map_err(|_| Error::InvalidJson)?;
    rows.as_array().cloned().ok_or(Error::InvalidResponse)
}

fn jitter(maximum_ms: u64) -> u64 {
    if maximum_ms == 0 {
        return 0;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    u64::from(nanos) % (maximum_ms + 1)
}
