//! Blocking downloads and CSV decoding for public Opportunity Insights datasets.

use std::{
    fs, io,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{blocking::Client as HttpClient, StatusCode};
use sha2::{Digest, Sha256};
use thiserror::Error;

const ATLAS_URL: &str =
    "https://opportunityinsights.org/wp-content/uploads/2018/10/tract_outcomes_simple.csv";
const SOCIAL_ZIP_URL: &str = "https://data.humdata.org/dataset/85ee8e10-0c66-4635-b997-79b6fad44c71/resource/ab878625-279b-4bef-a2b3-c132168d536e/download/social_capital_zip.csv";
const SOCIAL_COUNTY_URL: &str = "https://data.humdata.org/dataset/85ee8e10-0c66-4635-b997-79b6fad44c71/resource/ec896b64-c922-4737-b759-e4bd7f73b8cc/download/social_capital_county.csv";
const TRACKER_BASE: &str =
    "https://raw.githubusercontent.com/OpportunityInsights/EconomicTracker/main/data/";
const MAX_ATTEMPTS: u32 = 4;

/// A public CSV dataset hosted by Opportunity Insights or its named data provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dataset {
    /// Opportunity Atlas tract outcome estimates.
    AtlasTracts,
    /// Social Capital Atlas ZIP code estimates.
    SocialCapitalZip,
    /// Social Capital Atlas county estimates.
    SocialCapitalCounties,
    /// Economic Tracker observations for a county or state.
    EconomicTracker {
        /// Tracker series to download.
        series: TrackerSeries,
        /// Geography level to download.
        geography: TrackerGeography,
    },
}

/// Economic Tracker data series.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackerSeries {
    /// Affinity Solutions consumer spending data.
    ConsumerSpending,
    /// Womply small business revenue data.
    SmallBusinessRevenue,
    /// Employment data.
    Employment,
}

/// Economic Tracker geography level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackerGeography {
    /// County level data.
    County,
    /// State level data.
    State,
}

impl Dataset {
    /// Returns the upstream CSV URL for this dataset.
    #[must_use]
    pub fn url(self) -> String {
        match self {
            Self::AtlasTracts => ATLAS_URL.to_owned(),
            Self::SocialCapitalZip => SOCIAL_ZIP_URL.to_owned(),
            Self::SocialCapitalCounties => SOCIAL_COUNTY_URL.to_owned(),
            Self::EconomicTracker { series, geography } => {
                let geography = match geography {
                    TrackerGeography::County => "County",
                    TrackerGeography::State => "State",
                };
                let filename = match series {
                    TrackerSeries::ConsumerSpending => {
                        format!("Affinity - {geography} - Monthly.csv")
                    }
                    TrackerSeries::SmallBusinessRevenue => {
                        format!("Womply - {geography} - Weekly.csv")
                    }
                    TrackerSeries::Employment => format!("Employment - {geography} - Weekly.csv"),
                };
                format!("{TRACKER_BASE}{}", filename.replace(' ', "%20"))
            }
        }
    }
}

/// A decoded CSV header and its records, preserving all fields as text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CsvTable {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl CsvTable {
    /// Decodes CSV bytes while accepting rows with differing field counts.
    ///
    /// # Errors
    /// Returns an error when the input has no valid header or a malformed record.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let mut reader = csv::ReaderBuilder::new().flexible(true).from_reader(bytes);
        let headers = reader.headers()?.iter().map(str::to_owned).collect();
        let rows = reader
            .records()
            .map(|record| record.map(|row| row.iter().map(str::to_owned).collect()))
            .collect::<Result<Vec<Vec<String>>, csv::Error>>()?;
        Ok(Self { headers, rows })
    }

    /// Returns the column names in source order.
    #[must_use]
    pub fn headers(&self) -> &[String] {
        &self.headers
    }

    /// Returns all rows in source order.
    #[must_use]
    pub fn rows(&self) -> &[Vec<String>] {
        &self.rows
    }

    /// Finds a non-empty cell by case-insensitive column name.
    #[must_use]
    pub fn cell<'a>(&'a self, row: &'a [String], names: &[&str]) -> Option<&'a str> {
        names.iter().find_map(|name| {
            self.headers
                .iter()
                .position(|column| column.eq_ignore_ascii_case(name))
                .and_then(|index| row.get(index).map(String::as_str))
                .filter(|value| !value.is_empty())
        })
    }
}

/// HTTP, filesystem, or CSV decoding failure.
#[derive(Debug, Error)]
pub enum Error {
    /// HTTP client setup or transport failed.
    #[error("request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// The upstream returned an unsuccessful status after retries.
    #[error("upstream returned HTTP {0}")]
    Status(StatusCode),
    /// Retry attempts ended after transport failures.
    #[error("request retries exhausted for {0}")]
    RetriesExhausted(String),
    /// Cache read or write failed.
    #[error("cache I/O failed: {0}")]
    Cache(#[from] io::Error),
    /// The requested dataset is not in the cache while offline mode is active.
    #[error("offline cache miss for {0}")]
    OfflineCacheMiss(String),
    /// The response could not be decoded as CSV.
    #[error("CSV decode failed: {0}")]
    Csv(#[from] csv::Error),
}

/// Reusable client for public Opportunity Insights CSV downloads.
#[derive(Clone, Debug)]
pub struct Client {
    http: HttpClient,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl Client {
    /// Creates a client with a package-version user agent and no cache.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be configured.
    pub fn new() -> Result<Self, Error> {
        Self::builder().build()
    }

    /// Creates a client builder.
    #[must_use]
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Downloads and decodes a dataset, using and populating the optional cache.
    ///
    /// # Errors
    /// Returns an HTTP, cache, status, or CSV decoding error.
    pub fn fetch(&self, dataset: Dataset) -> Result<CsvTable, Error> {
        let url = dataset.url();
        let bytes = if let Some(cache_dir) = &self.cache_dir {
            let path = cache_dir.join(cache_key(&url));
            match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::NotFound && self.offline => {
                    return Err(Error::OfflineCacheMiss(url));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    let bytes = self.download(&url)?;
                    fs::create_dir_all(cache_dir)?;
                    let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
                    fs::write(&temporary, &bytes)?;
                    fs::rename(temporary, path)?;
                    bytes
                }
                Err(error) => return Err(error.into()),
            }
        } else if self.offline {
            return Err(Error::OfflineCacheMiss(url));
        } else {
            self.download(&url)?
        };
        CsvTable::from_bytes(&bytes)
    }

    fn download(&self, url: &str) -> Result<Vec<u8>, Error> {
        for attempt in 0..MAX_ATTEMPTS {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    return Ok(response.bytes()?.to_vec());
                }
                Ok(response) => {
                    let status = response.status();
                    if !retryable_status(status) || attempt + 1 == MAX_ATTEMPTS {
                        return Err(Error::Status(status));
                    }
                }
                Err(error) if attempt + 1 == MAX_ATTEMPTS => return Err(error.into()),
                Err(_) => {}
            }
            thread::sleep(retry_delay(attempt));
        }
        Err(Error::RetriesExhausted(url.to_owned()))
    }
}

/// Builder for [`Client`].
#[derive(Clone, Debug)]
pub struct ClientBuilder {
    user_agent: String,
    cache_dir: Option<PathBuf>,
    timeout: Duration,
    offline: bool,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            user_agent: concat!("statsapi-opportunity-insights/", env!("CARGO_PKG_VERSION")).into(),
            cache_dir: None,
            timeout: Duration::from_secs(90),
            offline: false,
        }
    }
}

impl ClientBuilder {
    /// Sets a caller-supplied cache directory.
    #[must_use]
    pub fn cache_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(path.as_ref().to_owned());
        self
    }

    /// Sets the HTTP user agent.
    #[must_use]
    pub fn user_agent(mut self, value: impl Into<String>) -> Self {
        self.user_agent = value.into();
        self
    }

    /// Sets the per-request timeout.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Requires a cache hit and prevents network access.
    #[must_use]
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    /// Builds the configured client.
    ///
    /// # Errors
    /// Returns an error if reqwest rejects the HTTP configuration.
    pub fn build(self) -> Result<Client, Error> {
        let http = HttpClient::builder()
            .user_agent(self.user_agent)
            .timeout(self.timeout)
            .build()?;
        Ok(Client {
            http,
            cache_dir: self.cache_dir,
            offline: self.offline,
        })
    }
}

fn retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn retry_delay(attempt: u32) -> Duration {
    let jitter = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis()
        % 251;
    Duration::from_millis(250 * u64::from(attempt + 1) + u64::from(jitter))
}

fn cache_key(url: &str) -> String {
    Sha256::digest(url.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut out, byte| {
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{byte:02x}"));
            out
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_layout_fixtures_decode_without_upstream_values() {
        for fixture in [
            include_bytes!("../tests/fixtures/atlas_tract.csv").as_slice(),
            include_bytes!("../tests/fixtures/social_capital_zip.csv").as_slice(),
            include_bytes!("../tests/fixtures/social_capital_county.csv").as_slice(),
            include_bytes!("../tests/fixtures/tracker_womply_county.csv").as_slice(),
        ] {
            let table = CsvTable::from_bytes(fixture).expect("synthetic CSV fixture decodes");
            assert!(
                !table.headers().is_empty(),
                "fixture includes a source header"
            );
            assert!(
                !table.rows().is_empty(),
                "fixture includes a synthetic record"
            );
        }
    }

    #[test]
    fn cell_lookup_is_case_insensitive_and_skips_empty_fields() {
        let table = CsvTable::from_bytes(b"Name,Value\nexample,\n").expect("fixture decodes");
        let row = &table.rows()[0];
        assert_eq!(table.cell(row, &["value", "NAME"]), Some("example"));
    }

    #[test]
    fn urls_cover_all_supported_upstream_files() {
        assert!(Dataset::AtlasTracts
            .url()
            .ends_with("tract_outcomes_simple.csv"));
        assert!(Dataset::SocialCapitalZip
            .url()
            .ends_with("social_capital_zip.csv"));
        assert!(Dataset::SocialCapitalCounties
            .url()
            .ends_with("social_capital_county.csv"));
        assert!(Dataset::EconomicTracker {
            series: TrackerSeries::SmallBusinessRevenue,
            geography: TrackerGeography::County,
        }
        .url()
        .ends_with("Womply%20-%20County%20-%20Weekly.csv"));
    }
}
