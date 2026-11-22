use crate::{NY_COUNTY_URL, NY_URL, URBAN_BASE};
use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Errors returned while fetching upstream household credit files.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// An HTTP request or response failed.
    #[error("upstream request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// A cache read or write failed.
    #[error("cache operation failed: {0}")]
    Io(#[from] io::Error),
    /// A requested endpoint is not one of the supported upstream resources.
    #[error("unsupported household credit resource")]
    UnsupportedResource,
    /// All retry attempts failed.
    #[error("upstream retries exhausted: {0}")]
    RetriesExhausted(String),
    /// Offline mode requested a response that was not already cached.
    #[error("offline cache miss for {0}")]
    OfflineCacheMiss(String),
}

/// Blocking, cache-optional HTTP client for the source files used by this crate.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl Client {
    /// Creates a client without a disk cache.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn new() -> Result<Self, Error> {
        Self::build(None)
    }

    /// Creates a client whose downloaded response bodies are cached under `cache_dir`.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn with_cache_dir(cache_dir: impl Into<PathBuf>) -> Result<Self, Error> {
        Self::build(Some(cache_dir.into()))
    }

    fn build(cache_dir: Option<PathBuf>) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!(
                "statsapi-household-credit/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()?;
        Ok(Self {
            http,
            cache_dir,
            offline: false,
        })
    }

    /// Uses only cached response bodies and returns an error for a cache miss.
    #[must_use]
    pub fn offline(mut self) -> Self {
        self.offline = true;
        self
    }

    /// Downloads one of the documented Urban, New York Fed, or Federal Reserve source URLs.
    ///
    /// Cached bodies are returned without a network request. Failures on transport errors,
    /// HTTP 429, and HTTP 5xx are retried three times with jittered exponential delays.
    ///
    /// # Errors
    /// Returns an error for unsupported URLs, exhausted retries, or cache I/O failures.
    pub fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        if !supported_url(url) {
            return Err(Error::UnsupportedResource);
        }
        let cache_path = self.cache_dir.as_ref().map(|dir| cache_path(dir, url));
        if let Some(path) = &cache_path {
            if let Ok(body) = std::fs::read(path) {
                return Ok(body);
            }
        }
        if self.offline {
            return Err(Error::OfflineCacheMiss(url.to_owned()));
        }

        let mut last_error = None;
        for attempt in 0..4 {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => match response.bytes() {
                    Ok(bytes) => {
                        let body = bytes.to_vec();
                        if let Some(path) = cache_path {
                            if let Some(parent) = path.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            let temporary =
                                path.with_extension(format!("tmp.{}", std::process::id()));
                            std::fs::write(&temporary, &body)?;
                            std::fs::rename(temporary, path)?;
                        }
                        return Ok(body);
                    }
                    Err(error) if attempt == 3 => return Err(Error::Http(error)),
                    Err(error) => last_error = Some(error.to_string()),
                },
                Ok(response) => {
                    let status = response.status();
                    if !retryable_status(status) {
                        return Err(Error::RetriesExhausted(status.to_string()));
                    }
                    if attempt == 3 {
                        return Err(Error::RetriesExhausted(status.to_string()));
                    }
                    last_error = Some(status.to_string());
                }
                Err(error) if attempt == 3 => return Err(Error::Http(error)),
                Err(error) => last_error = Some(error.to_string()),
            }
            thread::sleep(retry_delay(attempt));
        }
        Err(Error::RetriesExhausted(
            last_error.unwrap_or_else(|| "unknown upstream failure".into()),
        ))
    }

    /// Downloads a named Urban CSV file from the documented dataset release.
    ///
    /// # Errors
    /// Returns an error if the file name is unsupported or the request fails.
    pub fn urban_csv(&self, file: &str) -> Result<Vec<u8>, Error> {
        if !matches!(
            file,
            "county_overall.csv"
                | "county_medical.csv"
                | "county_auto.csv"
                | "county_student.csv"
                | "state_national_overall.csv"
                | "state_national_medical.csv"
                | "state_national_auto.csv"
                | "state_national_student.csv"
        ) {
            return Err(Error::UnsupportedResource);
        }
        self.fetch(&format!("{URBAN_BASE}/{file}"))
    }

    /// Downloads the current New York Fed Household Debt and Credit workbook.
    ///
    /// # Errors
    /// Returns an error if the request fails.
    pub fn ny_workbook(&self) -> Result<Vec<u8>, Error> {
        self.fetch(NY_URL)
    }

    /// Downloads the Federal Reserve Board county debt interval archive.
    ///
    /// # Errors
    /// Returns an error if the request fails.
    pub fn county_archive(&self) -> Result<Vec<u8>, Error> {
        self.fetch(NY_COUNTY_URL)
    }
}

fn supported_url(url: &str) -> bool {
    let is_csv = url.rsplit_once('/').is_some_and(|(_, file)| {
        file.rsplit_once('.')
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("csv"))
    });
    url.starts_with(&format!("{URBAN_BASE}/")) && is_csv || url == NY_URL || url == NY_COUNTY_URL
}

fn retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn retry_delay(attempt: u32) -> Duration {
    let base_ms = 250_u64.saturating_mul(2_u64.pow(attempt));
    let jitter = u64::from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_millis(),
    ) % (base_ms / 2 + 1);
    Duration::from_millis(base_ms + jitter)
}

fn cache_path(cache_dir: &Path, url: &str) -> PathBuf {
    let mut key = String::with_capacity(64);
    for byte in Sha256::digest(url.as_bytes()) {
        use std::fmt::Write as _;
        let _ = write!(key, "{byte:02x}");
    }
    cache_dir.join(&key[..2]).join(format!("{key}.body"))
}
