//! Blocking client for County Health Rankings & Roadmaps annual analytic CSV data.

use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Current published annual analytic CSV used by default.
pub const ANNUAL_CSV_URL: &str = "https://www.countyhealthrankings.org/sites/default/files/media/document/analytic_data2025_v3.csv";

const MAX_ATTEMPTS: usize = 4;

/// Errors returned while fetching or decoding County Health Rankings data.
#[derive(Debug)]
pub enum Error {
    /// A network, HTTP, cache, or filesystem operation failed.
    Request(String),
    /// The CSV did not contain a required column or was malformed.
    Csv(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(message) => write!(formatter, "request failed: {message}"),
            Self::Csv(message) => write!(formatter, "CSV decode failed: {message}"),
        }
    }
}

impl std::error::Error for Error {}

/// A value and its published confidence interval, if present.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measure {
    /// Parsed finite numeric measure value.
    pub value: f64,
    /// Lower confidence interval bound, when published.
    pub ci_low: Option<f64>,
    /// Upper confidence interval bound, when published.
    pub ci_high: Option<f64>,
}

/// Reusable client. A cache is used only when the caller supplies a directory.
#[derive(Clone, Debug, Default)]
pub struct Client {
    cache_dir: Option<PathBuf>,
}

impl Client {
    /// Create a client with an optional caller-owned cache directory.
    #[must_use]
    pub fn new(cache_dir: Option<PathBuf>) -> Self {
        Self { cache_dir }
    }

    /// Download the annual CSV, using an optional content-addressed cache.
    ///
    /// Set `CHR_DATA_OFFLINE=1` to disable network access and require a cache hit.
    ///
    /// # Errors
    ///
    /// Returns an error when the request fails, offline data is unavailable, or
    /// the cache cannot be read or written.
    pub fn fetch_annual_csv(&self) -> Result<Vec<u8>, Error> {
        let Some(cache_dir) = &self.cache_dir else {
            if std::env::var("CHR_DATA_OFFLINE").as_deref() == Ok("1") {
                return Err(Error::Request(
                    "offline mode requires a caller-supplied cache directory".to_owned(),
                ));
            }
            return fetch(ANNUAL_CSV_URL);
        };
        let key = sha256_hex(ANNUAL_CSV_URL.as_bytes());
        let path = cache_dir.join(&key[..2]).join(format!("{key}.body"));
        if let Ok(bytes) = std::fs::read(&path) {
            return Ok(bytes);
        }
        if std::env::var("CHR_DATA_OFFLINE").as_deref() == Ok("1") {
            return Err(Error::Request(format!(
                "offline cache miss for {ANNUAL_CSV_URL}"
            )));
        }
        let bytes = fetch(ANNUAL_CSV_URL)?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent).map_err(|error| Error::Request(error.to_string()))?;
        let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&temporary, &bytes).map_err(|error| Error::Request(error.to_string()))?;
        std::fs::rename(temporary, path).map_err(|error| Error::Request(error.to_string()))?;
        Ok(bytes)
    }

    /// Find a county or state row in the upstream-layout CSV.
    ///
    /// # Errors
    ///
    /// Returns an error when the CSV is malformed or lacks a required column.
    pub fn measure(
        &self,
        csv_bytes: &[u8],
        fips: &str,
        source_name: &str,
    ) -> Result<Option<Measure>, Error> {
        decode_measure(csv_bytes, fips, source_name)
    }
}

fn fetch(url: &str) -> Result<Vec<u8>, Error> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("statsapi-county-health-rankings/0.1")
        .build()
        .map_err(|error| Error::Request(error.to_string()))?;
    let mut last_error = String::new();
    for attempt in 0..MAX_ATTEMPTS {
        match client.get(url).send() {
            Ok(response) if response.status().is_success() => match response.bytes() {
                Ok(body) => return Ok(body.to_vec()),
                Err(error) => last_error = error.to_string(),
            },
            Ok(response) => {
                let status = response.status();
                last_error = format!("HTTP {status}");
                if !retryable_status(status) {
                    break;
                }
            }
            Err(error) => last_error = error.to_string(),
        }
        if attempt + 1 < MAX_ATTEMPTS {
            retry_delay(attempt);
        }
    }
    Err(Error::Request(last_error))
}

fn retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn retry_delay(attempt: usize) {
    let entropy = u64::from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos(),
    );
    let cap_ms = 100_u64.saturating_mul(1_u64 << attempt.min(6));
    let jitter_ms = entropy % (cap_ms + 1);
    thread::sleep(Duration::from_millis(jitter_ms));
}

fn decode_measure(
    csv_bytes: &[u8],
    fips: &str,
    source_name: &str,
) -> Result<Option<Measure>, Error> {
    let mut reader = csv::Reader::from_reader(csv_bytes);
    let headers = reader
        .headers()
        .map_err(|error| Error::Csv(error.to_string()))?
        .clone();
    let fips_index = headers
        .iter()
        .position(|header| header == "5-digit FIPS Code")
        .ok_or_else(|| Error::Csv("CSV missing 5-digit FIPS Code".to_owned()))?;
    let value_header = format!("{source_name} raw value");
    let low_header = format!("{source_name} CI low");
    let high_header = format!("{source_name} CI high");
    let value_index = headers
        .iter()
        .position(|header| header == value_header)
        .ok_or_else(|| Error::Csv(format!("CSV missing {value_header}")))?;
    let low_index = headers.iter().position(|header| header == low_header);
    let high_index = headers.iter().position(|header| header == high_header);
    for row in reader.records() {
        let row = row.map_err(|error| Error::Csv(error.to_string()))?;
        if row.get(fips_index) != Some(fips) {
            continue;
        }
        let Some(value) = row.get(value_index).and_then(parse_number) else {
            return Ok(None);
        };
        return Ok(Some(Measure {
            value,
            ci_low: low_index
                .and_then(|index| row.get(index))
                .and_then(parse_number),
            ci_high: high_index
                .and_then(|index| row.get(index))
                .and_then(parse_number),
        }));
    }
    Ok(None)
}

fn parse_number(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "NA" || trimmed == "." {
        return None;
    }
    trimmed
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            use fmt::Write as _;
            let _ = write!(output, "{byte:02x}");
            output
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/annual.csv");

    #[test]
    fn decodes_synthetic_county_and_state_rows() {
        let client = Client::default();
        assert_eq!(
            client.measure(FIXTURE, "34023", "Premature Death").unwrap(),
            Some(Measure {
                value: 4321.25,
                ci_low: Some(4100.0),
                ci_high: Some(4542.5)
            })
        );
        assert_eq!(
            client.measure(FIXTURE, "34000", "Premature Death").unwrap(),
            Some(Measure {
                value: 5678.5,
                ci_low: Some(5500.0),
                ci_high: Some(5857.0)
            })
        );
    }

    #[test]
    fn missing_and_non_numeric_values_are_unavailable() {
        let client = Client::default();
        assert_eq!(
            client.measure(FIXTURE, "99999", "Premature Death").unwrap(),
            None
        );
        assert_eq!(
            client.measure(FIXTURE, "34024", "Premature Death").unwrap(),
            None
        );
        assert_eq!(
            client
                .measure(FIXTURE, "34023", "Missing")
                .unwrap_err()
                .to_string(),
            "CSV decode failed: CSV missing Missing raw value"
        );
    }
}
