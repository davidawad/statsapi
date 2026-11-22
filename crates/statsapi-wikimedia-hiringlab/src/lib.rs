//! Blocking clients for Wikimedia pageviews and Indeed Hiring Lab postings.
//!
//! Requests identify the application with the caller-provided User-Agent.
//! A caller may provide a cache directory; no global cache or credentials are
//! used.

use reqwest::blocking::Client as HttpClient;
use reqwest::header::RETRY_AFTER;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use thiserror::Error;

pub const WIKIMEDIA_PAGEVIEWS_ENDPOINT: &str =
    "https://wikimedia.org/api/rest_v1/metrics/pageviews/per-article";
pub const WIKIDATA_SEARCH_ENDPOINT: &str = "https://www.wikidata.org/w/api.php";
pub const HIRING_LAB_COUNTRY_CSV: &str =
    "https://raw.githubusercontent.com/hiring-lab/job_postings_tracker/master/US/aggregate_job_postings_US.csv";
pub const HIRING_LAB_METRO_CSV: &str =
    "https://raw.githubusercontent.com/hiring-lab/job_postings_tracker/master/US/metro_job_postings_us.csv";

/// Errors returned by HTTP requests, decoding, or optional cache access.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// The HTTP request failed after retrying transient errors.
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    /// The upstream returned a non-success response.
    #[error("upstream returned HTTP status {0}")]
    HttpStatus(reqwest::StatusCode),
    /// The upstream response did not match its documented layout.
    #[error("invalid upstream response: {0}")]
    InvalidResponse(String),
    /// A CSV response could not be decoded.
    #[error("invalid CSV response: {0}")]
    Csv(#[from] csv::Error),
    /// A configured cache could not be read or written.
    #[error("cache I/O failed: {0}")]
    Cache(#[from] std::io::Error),
}

/// Result type returned by this client.
pub type Result<T> = std::result::Result<T, Error>;

/// A decoded pageview response with the total and the source URL.
#[derive(Clone, Debug, PartialEq)]
pub struct Pageviews {
    /// Sum of daily pageviews returned by the requested period.
    pub total: u64,
    /// URL that was requested.
    pub source_url: String,
}

/// A decoded Hiring Lab observation and its value roughly three months earlier.
#[derive(Clone, Debug, PartialEq)]
pub struct HiringLabObservation {
    /// Most recent matching observation value.
    pub latest: f64,
    /// Most recent matching observation date as provided by the CSV.
    pub latest_date: String,
    /// Last matching observation at least three months before the latest.
    pub three_months_prior: f64,
    /// Source URL with the selected geography appended as a query description.
    pub source_url: String,
}

/// Blocking Wikimedia and Indeed Hiring Lab client.
#[derive(Clone, Debug)]
pub struct Client {
    http: HttpClient,
    cache_dir: Option<PathBuf>,
    max_attempts: u32,
}

impl Client {
    /// Creates a client with a descriptive HTTP User-Agent.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be constructed.
    pub fn new(user_agent: &str) -> Result<Self> {
        Self::builder(user_agent).build()
    }

    /// Creates a builder for optional cache configuration.
    #[must_use]
    pub fn builder(user_agent: &str) -> ClientBuilder {
        ClientBuilder {
            user_agent: user_agent.to_owned(),
            cache_dir: None,
            max_attempts: 4,
        }
    }

    /// Resolves the first English Wikidata search result to its English Wikipedia title.
    ///
    /// # Errors
    /// Returns an error for HTTP failures or missing entity/sitelink fields.
    pub fn wikipedia_title(&self, label: &str) -> Result<String> {
        let bytes = self.get_bytes(
            WIKIDATA_SEARCH_ENDPOINT,
            &[
                ("action", "wbsearchentities"),
                ("search", label),
                ("language", "en"),
                ("uselang", "en"),
                ("format", "json"),
            ],
        )?;
        let search: Value = serde_json::from_slice(&bytes)
            .map_err(|error| Error::InvalidResponse(error.to_string()))?;
        let entity = search
            .pointer("/search/0/id")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::InvalidResponse("Wikidata search has no entity".into()))?;
        let entity_url = format!("https://www.wikidata.org/wiki/Special:EntityData/{entity}.json");
        let entity_bytes = self.get_bytes(&entity_url, &[])?;
        let data: Value = serde_json::from_slice(&entity_bytes)
            .map_err(|error| Error::InvalidResponse(error.to_string()))?;
        data.pointer(&format!("/entities/{entity}/sitelinks/enwiki/title"))
            .and_then(Value::as_str)
            .map(|title| title.replace(' ', "_"))
            .ok_or_else(|| {
                Error::InvalidResponse("Wikidata entity has no English Wikipedia title".into())
            })
    }

    /// Fetches and sums daily pageviews for an English Wikipedia article.
    /// Dates use Wikimedia's `YYYYMMDD` path format.
    ///
    /// # Errors
    /// Returns an error for HTTP failures or malformed response data.
    pub fn pageviews(&self, article: &str, start: &str, end: &str) -> Result<Pageviews> {
        let url = format!(
            "{WIKIMEDIA_PAGEVIEWS_ENDPOINT}/en.wikipedia.org/all-access/user/{article}/daily/{start}/{end}"
        );
        let bytes = self.get_bytes(&url, &[])?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| Error::InvalidResponse(error.to_string()))?;
        let items = value
            .get("items")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::InvalidResponse("pageviews response has no items".into()))?;
        let total = items
            .iter()
            .filter_map(|item| item.get("views").and_then(Value::as_u64))
            .sum();
        Ok(Pageviews {
            total,
            source_url: url,
        })
    }

    /// Fetches a Hiring Lab CSV and decodes an observation for the requested geography.
    ///
    /// `country_level` selects the published country aggregate schema; otherwise the
    /// published US metro schema is used. `geography` is `US` for the country file or a
    /// CBSA code for the metro file.
    ///
    /// # Errors
    /// Returns an error for HTTP/CSV failures or when observations are unavailable.
    pub fn hiring_lab_observation(
        &self,
        country_level: bool,
        geography: &str,
    ) -> Result<HiringLabObservation> {
        let (url, value_column, geography_column) = if country_level {
            (
                HIRING_LAB_COUNTRY_CSV,
                "indeed_job_postings_index_SA",
                "jobcountry",
            )
        } else {
            (
                HIRING_LAB_METRO_CSV,
                "indeed_job_postings_index",
                "cbsa_code",
            )
        };
        let bytes = self.get_bytes(url, &[])?;
        decode_hiring_lab_csv(&bytes, url, value_column, geography_column, geography)
    }

    fn get_bytes(&self, url: &str, query: &[(&str, &str)]) -> Result<Vec<u8>> {
        let request_url = if query.is_empty() {
            url.to_owned()
        } else {
            let request = self.http.get(url).query(query);
            let built = request.build()?;
            built.url().to_string()
        };
        if let Some(path) = self.cache_path(&request_url) {
            if path.is_file() {
                return Ok(fs::read(path)?);
            }
        }
        let mut last_error = None;
        for attempt in 0..self.max_attempts {
            let mut retry_after = None;
            let response = self.http.get(url).query(query).send();
            match response {
                Ok(response) if response.status().is_success() => match response.bytes() {
                    Ok(body) => {
                        let bytes = body.to_vec();
                        if let Some(path) = self.cache_path(&request_url) {
                            if let Some(parent) = path.parent() {
                                fs::create_dir_all(parent)?;
                            }
                            let temporary = path.with_extension(format!(
                                "{}.{}.tmp",
                                std::process::id(),
                                fastrand::u64(..)
                            ));
                            fs::write(&temporary, &bytes)?;
                            fs::rename(temporary, path)?;
                        }
                        return Ok(bytes);
                    }
                    Err(error) => last_error = Some(Error::Request(error)),
                },
                Ok(response) => {
                    let status = response.status();
                    retry_after = response
                        .headers()
                        .get(RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.parse::<u64>().ok());
                    if !(status.as_u16() == 429 || status.is_server_error()) {
                        return Err(Error::HttpStatus(status));
                    }
                    last_error = Some(Error::HttpStatus(status));
                }
                Err(error) => last_error = Some(Error::Request(error)),
            }
            if attempt + 1 < self.max_attempts {
                let base_ms = 200_u64.saturating_mul(1_u64 << attempt.min(8));
                let jitter_ms = fastrand::u64(0..=base_ms);
                thread::sleep(Duration::from_millis(
                    retry_after
                        .map_or(base_ms + jitter_ms, |seconds| seconds.saturating_mul(1_000)),
                ));
            }
        }
        Err(last_error
            .unwrap_or_else(|| Error::InvalidResponse("no request attempts configured".into())))
    }

    fn cache_path(&self, url: &str) -> Option<PathBuf> {
        self.cache_dir.as_ref().map(|directory| {
            let digest = Sha256::digest(url.as_bytes());
            directory.join(format!("{digest:x}.cache"))
        })
    }
}

/// Client builder with optional caller-owned caching.
#[derive(Clone, Debug)]
pub struct ClientBuilder {
    user_agent: String,
    cache_dir: Option<PathBuf>,
    max_attempts: u32,
}

impl ClientBuilder {
    /// Sets the optional cache directory. Cached responses are keyed by URL.
    #[must_use]
    pub fn cache_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Sets the maximum number of attempts for transient failures.
    #[must_use]
    pub fn max_attempts(mut self, attempts: u32) -> Self {
        self.max_attempts = attempts.max(1);
        self
    }

    /// Builds the client.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be constructed.
    pub fn build(self) -> Result<Client> {
        let http = HttpClient::builder()
            .user_agent(self.user_agent)
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Client {
            http,
            cache_dir: self.cache_dir,
            max_attempts: self.max_attempts,
        })
    }
}

/// Decodes a Hiring Lab CSV while preserving the upstream column layout.
///
/// # Errors
/// Returns an error if required columns or usable observations are missing.
pub fn decode_hiring_lab_csv(
    bytes: &[u8],
    source_url: &str,
    value_column: &str,
    geography_column: &str,
    geography: &str,
) -> Result<HiringLabObservation> {
    let mut reader = csv::Reader::from_reader(bytes);
    let headers = reader.headers()?.clone();
    let date_index = column_index(&headers, "date")?;
    let value_index = column_index(&headers, value_column)?;
    let geography_index = column_index(&headers, geography_column)?;
    let variable_index = headers.iter().position(|header| header == "variable");
    let mut rows = Vec::new();
    for row in reader.records() {
        let row = row?;
        if row.get(geography_index) != Some(geography)
            || variable_index.is_some_and(|index| row.get(index) != Some("total postings"))
        {
            continue;
        }
        let (Some(date), Some(value)) = (row.get(date_index), row.get(value_index)) else {
            continue;
        };
        if let Ok(value) = value.parse::<f64>() {
            rows.push((date.to_owned(), value));
        }
    }
    let (latest_date, latest) = rows.last().ok_or_else(|| {
        Error::InvalidResponse(format!(
            "no Hiring Lab observation for geography {geography}"
        ))
    })?;
    let latest_month = month_number(latest_date)
        .ok_or_else(|| Error::InvalidResponse("invalid Hiring Lab date".into()))?;
    let prior = rows
        .iter()
        .rev()
        .find(|(date, _)| month_number(date).is_some_and(|month| month <= latest_month - 3))
        .map(|(_, value)| *value)
        .ok_or_else(|| {
            Error::InvalidResponse("no Hiring Lab observation three months earlier".into())
        })?;
    Ok(HiringLabObservation {
        latest: *latest,
        latest_date: latest_date.clone(),
        three_months_prior: prior,
        source_url: source_url.to_owned(),
    })
}

fn column_index(headers: &csv::StringRecord, column: &str) -> Result<usize> {
    headers
        .iter()
        .position(|header| header == column)
        .ok_or_else(|| Error::InvalidResponse(format!("CSV has no {column} column")))
}

fn month_number(date: &str) -> Option<i32> {
    let (year, month) = date.get(..7)?.split_once('-')?;
    Some(year.parse::<i32>().ok()? * 12 + month.parse::<i32>().ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_synthetic_pageviews_layout() {
        let body = include_bytes!("../tests/fixtures/pageviews.json");
        let value: Value = serde_json::from_slice(body).unwrap();
        let total: u64 = value["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item["views"].as_u64())
            .sum();
        assert_eq!(total, 42);
    }

    #[test]
    fn decodes_synthetic_hiring_lab_country_fixture() {
        let fixture = include_bytes!("../tests/fixtures/hiring-lab-country.csv");
        let observation = decode_hiring_lab_csv(
            fixture,
            "fixture://country.csv",
            "indeed_job_postings_index_SA",
            "jobcountry",
            "US",
        )
        .unwrap();
        assert_eq!(observation.latest_date, "2024-04-01");
        assert_eq!(observation.latest, 104.0);
        assert_eq!(observation.three_months_prior, 101.0);
    }

    #[test]
    fn decodes_synthetic_hiring_lab_metro_fixture() {
        let fixture = include_bytes!("../tests/fixtures/hiring-lab-metro.csv");
        let observation = decode_hiring_lab_csv(
            fixture,
            "fixture://metro.csv",
            "indeed_job_postings_index",
            "cbsa_code",
            "12345",
        )
        .unwrap();
        assert_eq!(observation.latest_date, "2024-04-01");
        assert_eq!(observation.latest, 94.0);
        assert_eq!(observation.three_months_prior, 91.0);
    }

    #[test]
    fn decodes_synthetic_wikidata_layout() {
        let fixture = include_bytes!("../tests/fixtures/wikidata-entity.json");
        let entity: Value = serde_json::from_slice(fixture).unwrap();
        assert_eq!(
            entity
                .pointer("/entities/Q123/sitelinks/enwiki/title")
                .and_then(Value::as_str),
            Some("Example Town")
        );
    }

    #[test]
    #[ignore = "live smoke test; Wikimedia API is keyless"]
    fn live_pageviews_smoke_test() {
        let client = Client::new(
            "statsapi-wikimedia-hiringlab/0.1 (contact: https://github.com/davidawad/statsapi)",
        )
        .unwrap();
        let result = client
            .pageviews("Main_Page", "20261101", "20261102")
            .unwrap();
        assert!(result.total > 0);
        println!("Wikimedia Main_Page: {} views", result.total);
    }

    #[test]
    #[ignore = "live smoke test; Hiring Lab CSV is public and keyless"]
    fn live_hiring_lab_smoke_test() {
        let client = Client::new(
            "statsapi-wikimedia-hiringlab/0.1 (contact: https://github.com/davidawad/statsapi)",
        )
        .unwrap();
        let result = client.hiring_lab_observation(true, "US").unwrap();
        assert!(result.latest > 0.0);
        println!(
            "Hiring Lab US index: {} on {}",
            result.latest, result.latest_date
        );
    }
}
