//! Blocking access to Stanford Education Data Archive CSV releases.
//!
//! ```no_run
//! use statsapi_seda::{Client, Dataset, Level};
//!
//! # fn main() -> Result<(), statsapi_seda::Error> {
//! let client = Client::new()?.with_cache_dir("./seda-cache");
//! let county = client.fetch(Level::County, Dataset::Achievement, "06037")?;
//! println!("{} rows from {}", county.rows.len(), county.source_url);
//! # Ok(()) }
//! ```

use rand::Rng;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

pub use csv::StringRecord;

const RELEASE_2024: &str = "https://stacks.stanford.edu/file/druid:pt329xg7054/";
const RELEASE_6: &str = "https://stacks.stanford.edu/file/xh833nn4025/";
const MAX_ATTEMPTS: usize = 4;

/// SEDA geographic release level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Level {
    /// Administrative school district.
    District,
    /// County.
    County,
    /// State.
    State,
}

impl Level {
    fn id_column(self) -> &'static str {
        match self {
            Self::District => "sedaadmin",
            Self::County => "sedacounty",
            Self::State => "sedafips",
        }
    }
}

/// Dataset family available in the SEDA downloads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dataset {
    /// Achievement estimates and associated subgroup measures.
    Achievement,
    /// Growth or learning-rate estimates.
    Growth,
}

/// A downloaded, decoded, and geography-filtered source CSV.
#[derive(Clone, Debug)]
pub struct Table {
    /// Original CSV header row.
    pub headers: StringRecord,
    /// Rows matching the requested geography.
    pub rows: Vec<StringRecord>,
    /// Direct Stanford Digital Repository file URL.
    pub source_url: String,
    /// Human-readable release vintage.
    pub vintage: &'static str,
}

/// Client construction, HTTP, CSV, and cache errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// HTTP client configuration failed.
    #[error("HTTP client: {0}")]
    HttpClient(#[from] reqwest::Error),
    /// The upstream returned an unsuccessful status.
    #[error("upstream status: {0}")]
    HttpStatus(reqwest::StatusCode),
    /// The downloaded CSV was malformed or incomplete.
    #[error("CSV: {0}")]
    Csv(#[from] csv::Error),
    /// The requested file did not contain the expected identifier column.
    #[error("source file has no {0} column")]
    MissingIdColumn(&'static str),
    /// Cache read or write failed.
    #[error("cache: {0}")]
    Cache(#[from] std::io::Error),
    /// No cached copy exists while the client is in offline mode.
    #[error("offline cache miss")]
    OfflineCacheMiss,
}

/// Blocking SEDA client with optional caller-owned cache storage.
#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl Client {
    /// Create a client with a 280-second request timeout and no cache.
    ///
    /// # Errors
    ///
    /// Returns [`Error::HttpClient`] if the TLS-backed HTTP client cannot be configured.
    pub fn new() -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(280))
            .user_agent(concat!("statsapi-seda/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            cache_dir: None,
            offline: false,
        })
    }

    /// Store filtered source rows under this caller-supplied directory.
    #[must_use]
    pub fn with_cache_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Disable network access; `fetch` then succeeds only for cached files.
    #[must_use]
    pub fn offline_only(mut self) -> Self {
        self.offline = true;
        self
    }

    /// Download (or read cached) rows for one geography.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, the response is not a valid CSV,
    /// the requested identifier column is absent, or cache I/O fails.
    pub fn fetch(&self, level: Level, dataset: Dataset, code: &str) -> Result<Table, Error> {
        let (source_url, vintage) = source(level, dataset);
        let cache_path = self.cache_dir.as_ref().map(|root| {
            let key = Sha256::digest(format!("{source_url}:{code}").as_bytes());
            let hex = key.iter().fold(String::with_capacity(64), |mut out, byte| {
                use std::fmt::Write as _;
                let _ = write!(out, "{byte:02x}");
                out
            });
            root.join(format!("{hex}.csv"))
        });
        if let Some(path) = &cache_path {
            if path.exists() {
                return decode(&std::fs::read(path)?, level, code, source_url, vintage);
            }
        }

        if self.offline {
            return Err(Error::OfflineCacheMiss);
        }
        let bytes = self.request(&source_url)?;
        let table = decode(&bytes, level, code, source_url, vintage)?;
        if let Some(path) = cache_path {
            std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
            let mut writer = csv::WriterBuilder::new()
                .has_headers(false)
                .from_writer(Vec::new());
            writer.write_record(&table.headers)?;
            for row in &table.rows {
                writer.write_record(row)?;
            }
            let bytes = writer
                .into_inner()
                .map_err(|error| Error::Cache(error.into_error()))?;
            let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
            std::fs::write(&tmp, bytes)?;
            std::fs::rename(tmp, path)?;
        }
        Ok(table)
    }

    fn request(&self, url: &str) -> Result<Vec<u8>, Error> {
        for attempt in 0..MAX_ATTEMPTS {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => match response.bytes() {
                    Ok(bytes) => return Ok(bytes.to_vec()),
                    Err(_) if attempt + 1 < MAX_ATTEMPTS => retry_sleep(attempt),
                    Err(error) => return Err(Error::HttpClient(error)),
                },
                Ok(response)
                    if (response.status().as_u16() == 429
                        || response.status().is_server_error())
                        && attempt + 1 < MAX_ATTEMPTS =>
                {
                    retry_sleep(attempt);
                }
                Ok(response) => return Err(Error::HttpStatus(response.status())),
                Err(error) if attempt + 1 < MAX_ATTEMPTS => {
                    let _ = error;
                    retry_sleep(attempt);
                }
                Err(error) => return Err(Error::HttpClient(error)),
            }
        }
        unreachable!("retry loop always returns on its final attempt")
    }
}

fn retry_sleep(attempt: usize) {
    let ceiling_ms = 200_u64.saturating_mul(1_u64 << attempt.min(7));
    let delay = rand::thread_rng().gen_range(0..=ceiling_ms);
    thread::sleep(Duration::from_millis(delay));
}

fn source(level: Level, dataset: Dataset) -> (String, &'static str) {
    let suffix = if dataset == Dataset::Growth {
        "gcs"
    } else {
        "cs"
    };
    match level {
        Level::District => (
            format!("{RELEASE_2024}seda_admindist_annualsub_{suffix}_2024.1.csv"),
            "SEDA 2024.1",
        ),
        Level::State => (
            format!("{RELEASE_2024}seda_state_annualsub_{suffix}_2024.1.csv"),
            "SEDA 2024.1",
        ),
        Level::County => (
            format!("{RELEASE_6}seda_county_poolsub_{suffix}_6.0.csv"),
            "SEDA 6.0",
        ),
    }
}

fn decode(
    bytes: &[u8],
    level: Level,
    code: &str,
    source_url: String,
    vintage: &'static str,
) -> Result<Table, Error> {
    let mut reader = csv::ReaderBuilder::new().from_reader(bytes);
    let headers = reader.headers()?.clone();
    let id_index = headers
        .iter()
        .position(|header| header == level.id_column())
        .ok_or(Error::MissingIdColumn(level.id_column()))?;
    let mut rows = Vec::new();
    for row in reader.records() {
        let row = row?;
        if row.get(id_index) == Some(code) {
            rows.push(row);
        }
    }
    Ok(Table {
        headers,
        rows,
        source_url,
        vintage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_only_matching_county_rows_from_synthetic_layout() {
        let csv = b"sedacounty,sedacountyname,subcat,subgroup,cs_mn_avg_mth_ol,cs_mn_avg_rla_ol\n00001,Example County,all,all,0.2,0.4\n00002,Other County,all,all,0.3,0.5\n";
        let table = decode(
            csv,
            Level::County,
            "00001",
            "fixture://county-cs.csv".to_owned(),
            "synthetic",
        )
        .expect("synthetic source layout decodes");
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.rows[0].get(1), Some("Example County"));
    }

    #[test]
    fn reports_missing_geography_column() {
        let error = decode(
            b"id,value\n1,2\n",
            Level::County,
            "1",
            String::new(),
            "test",
        );
        assert!(matches!(error, Err(Error::MissingIdColumn("sedacounty"))));
    }

    #[test]
    fn reads_synthetic_release_layout_fixtures() {
        for (fixture, dataset, expected_rows) in [
            (
                include_bytes!("../tests/fixtures/county-cs.csv").as_slice(),
                Dataset::Achievement,
                6,
            ),
            (
                include_bytes!("../tests/fixtures/county-gcs.csv").as_slice(),
                Dataset::Growth,
                1,
            ),
        ] {
            let (url, vintage) = source(Level::County, dataset);
            let table = decode(fixture, Level::County, "00001", url, vintage)
                .expect("synthetic SEDA layout parses");
            assert_eq!(table.rows.len(), expected_rows);
            assert!(table.rows.iter().all(|row| row.get(0) == Some("00001")));
        }
    }
}
