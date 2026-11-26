//! Blocking client for downloading and decoding public HMDA CSV records.

use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use rand::Rng;
use reqwest::blocking::Client as HttpClient;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use thiserror::Error;

const CSV_ENDPOINT: &str = "https://ffiec.cfpb.gov/v2/data-browser-api/view/csv";

/// A decoded row with the fields used in common HMDA analyses.
#[derive(Clone, Debug, PartialEq)]
pub struct Application {
    /// Census tract identifier as published by HMDA.
    pub census_tract: String,
    /// HMDA action taken code.
    pub action_taken: String,
    /// Loan amount in dollars, if reported.
    pub loan_amount: Option<f64>,
    /// HMDA-derived applicant race category.
    pub derived_race: String,
    /// HMDA-derived applicant ethnicity category.
    pub derived_ethnicity: String,
    /// HOEPA status code.
    pub hoepa_status: String,
    /// Loan purpose code.
    pub loan_purpose: String,
}

/// CSV download parameters. Supply one supported geography filter and the filing year.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvRequest {
    /// Filing year (2018 through the years currently exposed by the API).
    pub year: u16,
    /// Geography filters such as `counties=01001`, `states=AL`, or `msamds=33860`.
    pub filters: BTreeMap<String, String>,
}

impl CsvRequest {
    /// Create a request for one filing year and geography filter.
    #[must_use]
    pub fn new(year: u16, filter: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            year,
            filters: BTreeMap::from([(filter.into(), value.into())]),
        }
    }
}

/// Errors returned while building, requesting, caching, or decoding HMDA data.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// The blocking HTTP client could not be constructed.
    #[error("could not build HTTP client: {0}")]
    HttpClient(#[source] reqwest::Error),
    /// A network request failed after retrying transient failures.
    #[error("HMDA request failed: {0}")]
    Request(#[source] reqwest::Error),
    /// The upstream rejected a request.
    #[error("HMDA returned HTTP status {0}")]
    HttpStatus(reqwest::StatusCode),
    /// The requested response did not contain the expected HMDA CSV columns.
    #[error("invalid HMDA CSV: {0}")]
    Csv(String),
    /// A cache entry could not be read or written.
    #[error("HMDA cache I/O failed: {0}")]
    Cache(#[source] std::io::Error),
    /// Offline mode was requested but no cache entry exists.
    #[error("offline cache miss for {0}")]
    OfflineMiss(String),
}

/// Configuration for [`Client`].
#[derive(Clone, Debug, Default)]
pub struct ClientBuilder {
    cache_dir: Option<PathBuf>,
    offline: bool,
    timeout: Option<Duration>,
}

impl ClientBuilder {
    /// Set an optional caller-owned cache directory.
    #[must_use]
    pub fn cache_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(path.into());
        self
    }

    /// Require a cached response and never contact the network.
    #[must_use]
    pub fn offline(mut self) -> Self {
        self.offline = true;
        self
    }

    /// Set the per-request timeout.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Build a client.
    ///
    /// # Errors
    ///
    /// Returns [`Error::HttpClient`] if the HTTP client cannot be constructed.
    pub fn build(self) -> Result<Client, Error> {
        let mut http = HttpClient::builder().user_agent("curl/8.0 (hmda-data public client)");
        if let Some(timeout) = self.timeout {
            http = http.timeout(timeout);
        }
        let http = http.build().map_err(Error::HttpClient)?;
        Ok(Client {
            http,
            cache_dir: self.cache_dir,
            offline: self.offline,
        })
    }
}

/// Client for the public FFIEC HMDA Data Browser API.
#[derive(Clone, Debug)]
pub struct Client {
    http: HttpClient,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl Client {
    /// Start configuring a client.
    #[must_use]
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Download and decode CSV rows matching `request`.
    ///
    /// # Errors
    ///
    /// Returns an error for network, HTTP, cache, offline, or CSV failures.
    pub fn applications(&self, request: &CsvRequest) -> Result<Vec<Application>, Error> {
        let url = request_url(request)?;
        let body = self.csv_bytes(&url)?;
        decode_csv(&body)
    }

    fn csv_bytes(&self, url: &str) -> Result<Vec<u8>, Error> {
        let cache_path = self.cache_dir.as_ref().map(|directory| {
            let hash = Sha256::digest(url.as_bytes());
            let filename = hash
                .iter()
                .fold(String::with_capacity(64), |mut out, byte| {
                    let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{byte:02x}"));
                    out
                });
            directory.join(format!("{filename}.csv.gz"))
        });
        if let Some(path) = cache_path.as_ref() {
            if let Ok(file) = std::fs::File::open(path) {
                let mut bytes = Vec::new();
                if GzDecoder::new(file).read_to_end(&mut bytes).is_ok() {
                    return Ok(bytes);
                }
            }
        }
        if self.offline {
            return Err(Error::OfflineMiss(url.to_owned()));
        }
        let response = self.request_with_retry(url)?;
        let bytes = response.bytes().map_err(Error::Request)?.to_vec();
        if let Some(path) = cache_path {
            let Some(parent) = path.parent() else {
                return Err(Error::Cache(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "cache path has no parent directory",
                )));
            };
            std::fs::create_dir_all(parent).map_err(Error::Cache)?;
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&bytes).map_err(Error::Cache)?;
            let compressed = encoder.finish().map_err(Error::Cache)?;
            let temp = path.with_extension(format!("tmp.{}", std::process::id()));
            std::fs::write(&temp, compressed).map_err(Error::Cache)?;
            std::fs::rename(temp, path).map_err(Error::Cache)?;
        }
        Ok(bytes)
    }

    fn request_with_retry(&self, url: &str) -> Result<reqwest::blocking::Response, Error> {
        let mut last_error = None;
        for attempt in 0..4_u32 {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response)
                    if response.status().is_server_error() || response.status().as_u16() == 429 =>
                {
                    let status = response.status();
                    last_error = Some(Error::HttpStatus(status));
                }
                Ok(response) => return Err(Error::HttpStatus(response.status())),
                Err(error) if error.is_timeout() || error.is_connect() => {
                    last_error = Some(Error::Request(error));
                }
                Err(error) => return Err(Error::Request(error)),
            }
            if attempt < 3 {
                let cap_ms = 250_u64.saturating_mul(2_u64.pow(attempt)).min(4_000);
                let delay = rand::thread_rng().gen_range(0..=cap_ms);
                thread::sleep(Duration::from_millis(delay));
            }
        }
        Err(last_error.unwrap_or_else(|| Error::Csv("no request attempt was made".into())))
    }
}

fn request_url(request: &CsvRequest) -> Result<String, Error> {
    let mut url = reqwest::Url::parse(CSV_ENDPOINT)
        .map_err(|error| Error::Csv(format!("invalid endpoint URL: {error}")))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("years", &request.year.to_string());
        for (key, value) in &request.filters {
            query.append_pair(key, value);
        }
    }
    Ok(url.to_string())
}

/// Decode the fields used by common HMDA analyses from a CSV response.
///
/// # Errors
///
/// Returns [`Error::Csv`] when required columns or valid CSV records are missing.
pub fn decode_csv(bytes: &[u8]) -> Result<Vec<Application>, Error> {
    let mut reader = csv::Reader::from_reader(bytes);
    let headers = reader
        .headers()
        .map_err(|error| Error::Csv(error.to_string()))?
        .clone();
    let column = |name: &str| {
        headers
            .iter()
            .position(|header| header == name)
            .ok_or_else(|| Error::Csv(format!("missing {name} column")))
    };
    let tract = column("census_tract")?;
    let action = column("action_taken")?;
    let amount = column("loan_amount")?;
    let race = column("derived_race")?;
    let ethnicity = column("derived_ethnicity")?;
    let hoepa = column("hoepa_status")?;
    let purpose = column("loan_purpose")?;
    reader
        .records()
        .map(|record| {
            let record = record.map_err(|error| Error::Csv(error.to_string()))?;
            Ok(Application {
                census_tract: field(&record, tract),
                action_taken: field(&record, action),
                loan_amount: record.get(amount).and_then(|value| value.parse().ok()),
                derived_race: field(&record, race),
                derived_ethnicity: field(&record, ethnicity),
                hoepa_status: field(&record, hoepa),
                loan_purpose: field(&record, purpose),
            })
        })
        .collect()
}

fn field(record: &csv::StringRecord, index: usize) -> String {
    record.get(index).unwrap_or_default().to_owned()
}
