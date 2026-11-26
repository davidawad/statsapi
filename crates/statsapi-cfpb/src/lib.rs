//! Blocking access to CFPB public complaint and consumer credit trends data.

use reqwest::blocking::Client as HttpClient;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const USER_AGENT: &str = "curl/8.14.1 (cfpb-data/0.1)";
const MAX_ATTEMPTS: u32 = 4;

/// A reusable blocking client for CFPB complaint and consumer credit trend sources.
#[derive(Clone, Debug)]
pub struct CfpbClient {
    http: HttpClient,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

/// Parsed complaint totals and product shares from the CFPB search API.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComplaintSummary {
    /// Number of matching complaints.
    pub total: u64,
    /// Share belonging to the largest product category.
    pub top_product_share: f64,
    /// Share belonging to debt collection.
    pub debt_collection_share: f64,
}

/// Errors returned by CFPB requests, cache access, and response decoding.
#[derive(Debug)]
pub enum Error {
    /// HTTP transport or response status failed.
    Http(reqwest::Error),
    /// Local cache read or write failed.
    Io(std::io::Error),
    /// Complaint JSON could not be decoded.
    Json(serde_json::Error),
    /// Consumer Credit Trends CSV could not be decoded.
    Csv(csv::Error),
    /// A response did not have the expected CFPB layout.
    InvalidResponse(&'static str),
    /// No row for the requested state was present.
    StateNotFound(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(error) => write!(formatter, "CFPB request failed: {error}"),
            Self::Io(error) => write!(formatter, "CFPB cache failed: {error}"),
            Self::Json(error) => write!(formatter, "invalid CFPB complaint JSON: {error}"),
            Self::Csv(error) => write!(formatter, "invalid CFPB state map CSV: {error}"),
            Self::InvalidResponse(reason) => write!(formatter, "invalid CFPB response: {reason}"),
            Self::StateNotFound(state) => {
                write!(formatter, "no Consumer Credit Trends row for {state}")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Csv(error) => Some(error),
            Self::InvalidResponse(_) | Self::StateNotFound(_) => None,
        }
    }
}

impl CfpbClient {
    /// Create a client with an optional caller-owned response cache directory.
    ///
    /// Requests use the network unless a matching cache file exists. The cache directory is
    /// created as needed and entries are keyed by the SHA-256 digest of the complete URL.
    ///
    /// # Errors
    /// Returns an error if the underlying HTTP client cannot be configured.
    pub fn new(cache_dir: Option<&Path>) -> Result<Self, Error> {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(45))
            .user_agent(USER_AGENT)
            .build()
            .map_err(Error::Http)?;
        Ok(Self {
            http,
            cache_dir: cache_dir.map(Path::to_path_buf),
            offline: false,
        })
    }

    /// Configure the client to return cache misses without making a network request.
    #[must_use]
    pub fn offline(mut self) -> Self {
        self.offline = true;
        self
    }

    /// Fetch and decode a CFPB Consumer Complaint Database API response.
    ///
    /// The URL should target the CFPB search API and include the desired filters and
    /// aggregations. Transient transport errors, HTTP 429, and 5xx responses are retried.
    ///
    /// # Errors
    /// Returns an error for network, cache, HTTP, or response decoding failures.
    pub fn complaints(&self, url: &str) -> Result<ComplaintSummary, Error> {
        parse_complaints(&self.get(url)?)
    }

    /// Fetch a CFPB Consumer Credit Trends state-map CSV and return one state's value.
    ///
    /// # Errors
    /// Returns an error for network, cache, CSV, or missing-state failures.
    pub fn state_map_value(&self, url: &str, state: &str) -> Result<f64, Error> {
        parse_state_map(&self.get(url)?, state)
    }

    fn get(&self, url: &str) -> Result<Vec<u8>, Error> {
        if let Some(root) = &self.cache_dir {
            let path = root.join(cache_name(url));
            if let Ok(body) = fs::read(&path) {
                return Ok(body);
            }
            if self.offline {
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("offline cache miss for {url}"),
                )));
            }
            let body = self.request(url)?;
            fs::create_dir_all(root).map_err(Error::Io)?;
            let temp = root.join(format!(".{}.{}.tmp", cache_name(url), std::process::id()));
            fs::write(&temp, &body).map_err(Error::Io)?;
            fs::rename(temp, path).map_err(Error::Io)?;
            return Ok(body);
        }
        if self.offline {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "offline mode requires a cache directory",
            )));
        }
        self.request(url)
    }

    fn request(&self, url: &str) -> Result<Vec<u8>, Error> {
        let mut last_error = None;
        for attempt in 0..MAX_ATTEMPTS {
            match self.http.get(url).send() {
                Ok(response)
                    if is_retryable(response.status().as_u16()) && attempt + 1 < MAX_ATTEMPTS =>
                {
                    if let Err(error) = response.error_for_status() {
                        last_error = Some(Error::Http(error));
                    }
                    thread::sleep(retry_delay(attempt));
                }
                Ok(response) => {
                    return response
                        .error_for_status()
                        .map_err(Error::Http)?
                        .bytes()
                        .map(|bytes| bytes.to_vec())
                        .map_err(Error::Http);
                }
                Err(error) if attempt + 1 < MAX_ATTEMPTS => {
                    last_error = Some(Error::Http(error));
                    thread::sleep(retry_delay(attempt));
                }
                Err(error) => return Err(Error::Http(error)),
            }
        }
        Err(last_error.unwrap_or(Error::InvalidResponse(
            "retry loop ended without a response",
        )))
    }
}

/// Decode the complaint total and product buckets in a CFPB API JSON response.
///
/// # Errors
/// Returns an error if required total or aggregation fields are absent or malformed.
pub fn parse_complaints(body: &[u8]) -> Result<ComplaintSummary, Error> {
    let doc: Value = serde_json::from_slice(body).map_err(Error::Json)?;
    let total = doc
        .pointer("/hits/total/value")
        .and_then(Value::as_u64)
        .or_else(|| doc.pointer("/hits/total").and_then(Value::as_u64))
        .ok_or(Error::InvalidResponse("complaint response has no total"))?;
    let buckets = doc
        .pointer("/aggregations/product/product/buckets")
        .and_then(Value::as_array)
        .ok_or(Error::InvalidResponse(
            "complaint response has no product aggregation",
        ))?;
    if total == 0 {
        return Ok(ComplaintSummary {
            total,
            top_product_share: 0.0,
            debt_collection_share: 0.0,
        });
    }
    let max_count = buckets
        .iter()
        .filter_map(|bucket| bucket.get("doc_count").and_then(Value::as_u64))
        .max()
        .unwrap_or(0);
    let debt_count = buckets
        .iter()
        .find(|bucket| bucket.get("key").and_then(Value::as_str) == Some("Debt collection"))
        .and_then(|bucket| bucket.get("doc_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total = u32::try_from(total).map_err(|_| {
        Error::InvalidResponse("complaint count exceeds supported numeric precision")
    })?;
    let max_count = u32::try_from(max_count)
        .map_err(|_| Error::InvalidResponse("product count exceeds supported numeric precision"))?;
    let debt_count = u32::try_from(debt_count)
        .map_err(|_| Error::InvalidResponse("debt count exceeds supported numeric precision"))?;
    Ok(ComplaintSummary {
        total: u64::from(total),
        top_product_share: f64::from(max_count) / f64::from(total),
        debt_collection_share: f64::from(debt_count) / f64::from(total),
    })
}

/// Decode a state-level value from a CFPB Consumer Credit Trends map CSV.
///
/// # Errors
/// Returns an error if the CSV is malformed or the state has no numeric value.
pub fn parse_state_map(body: &[u8], state: &str) -> Result<f64, Error> {
    let mut reader = csv::Reader::from_reader(body);
    for row in reader.records() {
        let row = row.map_err(Error::Csv)?;
        if row.get(1) == Some(state) {
            return row
                .get(2)
                .and_then(|value| value.parse().ok())
                .ok_or(Error::InvalidResponse("state map row has no numeric value"));
        }
    }
    Err(Error::StateNotFound(state.to_owned()))
}

fn cache_name(url: &str) -> String {
    use std::fmt::Write as _;

    let digest = Sha256::digest(url.as_bytes());
    let mut name = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(name, "{byte:02x}");
    }
    name
}

fn is_retryable(status: u16) -> bool {
    status == 429 || (500..=599).contains(&status)
}

fn retry_delay(attempt: u32) -> Duration {
    let base_ms = 250_u64.saturating_mul(2_u64.pow(attempt)).min(4_000);
    let jitter_seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let jitter_range = u32::try_from(base_ms / 2 + 1).unwrap_or(u32::MAX);
    let jitter_ms = u64::from(jitter_seed % jitter_range);
    Duration::from_millis(base_ms / 2 + jitter_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synthetic_upstream_layout_fixtures() {
        let summary = parse_complaints(include_bytes!("../tests/fixtures/complaints.json"))
            .expect("complaint fixture parses");
        assert_eq!(summary.total, 13);
        assert_eq!(summary.top_product_share, 8.0 / 13.0);
        assert_eq!(summary.debt_collection_share, 3.0 / 13.0);
        assert_eq!(
            parse_state_map(include_bytes!("../tests/fixtures/auto-map.csv"), "XY")
                .expect("state value parses"),
            0.125
        );
    }
}
