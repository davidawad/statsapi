//! Blocking access to Stanford Education Data Archive CSV releases.
//!
//! The release CSVs are sorted by geography id, so one geography is read with
//! HTTP range requests (a bisection, then a short stream of its rows); a server
//! that ignores `Range` gets the whole file streamed and filtered while it
//! downloads. Nothing is written to disk and no archive is kept.
//!
//! ```no_run
//! use statsapi_seda::{Client, Dataset, Level};
//!
//! # fn main() -> Result<(), statsapi_seda::Error> {
//! let client = Client::new()?;
//! let county = client.fetch(Level::County, Dataset::Achievement, "06037")?;
//! println!("{} rows from {}", county.rows.len(), county.source_url);
//! # Ok(()) }
//! ```

mod ranged;

use ephemeral_cache::{Source, Trail};
use rand::Rng;
use std::io::Read;
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
    /// Reading the response stream failed.
    #[error("stream: {0}")]
    Stream(#[from] std::io::Error),
}

/// Blocking SEDA client that streams and filters; it never persists anything.
#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
    trail: Trail,
    /// Replaces the Stanford release base URLs (tests).
    base: Option<String>,
}

impl Client {
    /// Create a client with a 280-second request timeout.
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
            trail: Trail::new(),
            base: None,
        })
    }

    /// Serve both releases from `base` instead of Stanford (for a local test server).
    #[must_use]
    pub fn with_release_base(mut self, base: impl Into<String>) -> Self {
        self.base = Some(base.into());
        self
    }

    /// The access paths this client has answered with (always `stream`).
    #[must_use]
    pub fn trail(&self) -> Trail {
        self.trail.clone()
    }

    /// Stream one release CSV and keep only the rows for one geography.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, the response is not a valid CSV,
    /// or the requested identifier column is absent.
    pub fn fetch(&self, level: Level, dataset: Dataset, code: &str) -> Result<Table, Error> {
        let (source_url, vintage) = source(level, dataset, self.base.as_deref());
        // The files are sorted by id: read just this geography's rows with range
        // requests, and stream the whole file only if the server cannot do that.
        for attempt in 0..MAX_ATTEMPTS {
            match ranged::fetch(&self.http, &source_url, level.id_column(), code, vintage) {
                Ok(Some(table)) => {
                    self.trail.note(Source::Stream);
                    return Ok(table);
                }
                Ok(None) => break,
                Err(Error::HttpClient(_) | Error::Stream(_) | Error::Csv(_))
                    if attempt + 1 < MAX_ATTEMPTS =>
                {
                    retry_sleep(attempt);
                }
                Err(error) => return Err(error),
            }
        }
        for attempt in 0..MAX_ATTEMPTS {
            let last = attempt + 1 == MAX_ATTEMPTS;
            match self.http.get(&source_url).send() {
                Ok(response) if response.status().is_success() => {
                    match decode(response, level, code, source_url.clone(), vintage) {
                        Ok(table) => {
                            self.trail.note(Source::Stream);
                            return Ok(table);
                        }
                        Err(Error::Stream(_) | Error::Csv(_)) if !last => retry_sleep(attempt),
                        Err(error) => return Err(error),
                    }
                }
                Ok(response)
                    if (response.status().as_u16() == 429
                        || response.status().is_server_error())
                        && !last =>
                {
                    retry_sleep(attempt);
                }
                Ok(response) => return Err(Error::HttpStatus(response.status())),
                Err(_) if !last => retry_sleep(attempt),
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

fn source(level: Level, dataset: Dataset, base: Option<&str>) -> (String, &'static str) {
    let suffix = if dataset == Dataset::Growth {
        "gcs"
    } else {
        "cs"
    };
    let root = |release: &'static str| base.unwrap_or(release).to_owned();
    match level {
        Level::District => (
            format!(
                "{}seda_admindist_annualsub_{suffix}_2024.1.csv",
                root(RELEASE_2024)
            ),
            "SEDA 2024.1",
        ),
        Level::State => (
            format!(
                "{}seda_state_annualsub_{suffix}_2024.1.csv",
                root(RELEASE_2024)
            ),
            "SEDA 2024.1",
        ),
        Level::County => (
            format!("{}seda_county_poolsub_{suffix}_6.0.csv", root(RELEASE_6)),
            "SEDA 6.0",
        ),
    }
}

fn decode(
    source: impl Read,
    level: Level,
    code: &str,
    source_url: String,
    vintage: &'static str,
) -> Result<Table, Error> {
    let mut reader = csv::ReaderBuilder::new().from_reader(source);
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
            csv.as_slice(),
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
            b"id,value\n1,2\n".as_slice(),
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
            let (url, vintage) = source(Level::County, dataset, None);
            let table = decode(fixture, Level::County, "00001", url, vintage)
                .expect("synthetic SEDA layout parses");
            assert_eq!(table.rows.len(), expected_rows);
            assert!(table.rows.iter().all(|row| row.get(0) == Some("00001")));
        }
    }

    #[test]
    fn decodes_a_response_delivered_in_small_chunks() {
        struct Trickle<'a>(&'a [u8]);
        impl Read for Trickle<'_> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let n = self.0.len().min(buf.len()).min(7);
                buf[..n].copy_from_slice(&self.0[..n]);
                self.0 = &self.0[n..];
                Ok(n)
            }
        }
        let csv = include_bytes!("../tests/fixtures/county-cs.csv");
        let table = decode(
            Trickle(csv),
            Level::County,
            "00001",
            String::new(),
            "synthetic",
        )
        .expect("chunked stream decodes");
        assert_eq!(table.rows.len(), 6);
    }
}
