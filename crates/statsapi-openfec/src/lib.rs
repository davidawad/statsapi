//! Blocking client for `OpenFEC` aggregate and committee data.
//!
//! The client deliberately does not expose individual contributor records.

use rand::Rng;
use reqwest::blocking::Client as HttpClient;
use reqwest::{StatusCode, Url};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

/// `OpenFEC` REST API base URL.
pub const API_BASE: &str = "https://api.open.fec.gov/v1/";
/// Environment variable read by [`Client::from_env`].
pub const API_KEY_ENV: &str = "API_DATA_GOV_KEY";
const MAX_RETRIES: u32 = 4;
const PAGE_SIZE: u16 = 100;
const MAX_PAGES: u16 = 500;
/// Pages fetched at once after the first; keeps well under the api.data.gov hourly quota per run.
const PAGE_CONCURRENCY: usize = 8;

/// Authenticated, blocking client for aggregate schedules and committee metadata.
pub struct Client {
    api_key: String,
    http: HttpClient,
    cache_dir: Option<PathBuf>,
    offline_only: bool,
    base_url: Url,
}

impl fmt::Debug for Client {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Client")
            .field("api_key", &"[REDACTED]")
            .field("cache_dir", &self.cache_dir)
            .field("offline_only", &self.offline_only)
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Creates a client using an API key supplied by the caller.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`] for an empty key or a transport error if
    /// the HTTP client cannot be constructed.
    pub fn new(api_key: impl Into<String>) -> Result<Self, Error> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(Error::MissingKey);
        }
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(45))
            .build()
            .map_err(Error::Transport)?;
        let base_url = Url::parse(API_BASE).map_err(|error| Error::Url(error.to_string()))?;
        Ok(Self {
            api_key,
            http,
            cache_dir: None,
            offline_only: false,
            base_url,
        })
    }

    /// Creates a client from `API_DATA_GOV_KEY`, returning [`Error::MissingKey`] when absent.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`] when the environment variable is absent or empty.
    pub fn from_env() -> Result<Self, Error> {
        let api_key = std::env::var(API_KEY_ENV).map_err(|_| Error::MissingKey)?;
        Self::new(api_key)
    }

    /// Sets an optional caller-owned directory for cached response bodies.
    #[must_use]
    pub fn with_cache_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(path.into());
        self
    }

    /// Restricts requests to the configured response cache.
    #[must_use]
    pub fn offline_only(mut self) -> Self {
        self.offline_only = true;
        self
    }

    /// Fetches aggregated Schedule A rows and committee metadata for a ZIP code and cycle.
    ///
    /// # Errors
    ///
    /// Returns an error for failed requests, malformed responses, or cache misses in offline mode.
    pub fn schedule_a_by_zip(&self, zip: &str, cycle: u16) -> Result<ScheduleAggregate, Error> {
        self.schedule_a("by_zip", "zip", zip, cycle)
    }

    /// Fetches aggregated Schedule A rows and committee metadata for a state and cycle.
    ///
    /// # Errors
    ///
    /// Returns an error for failed requests, malformed responses, or cache misses in offline mode.
    pub fn schedule_a_by_state(&self, state: &str, cycle: u16) -> Result<ScheduleAggregate, Error> {
        self.schedule_a("by_state", "state", state, cycle)
    }

    fn schedule_a(
        &self,
        endpoint: &str,
        geography_field: &str,
        geography: &str,
        cycle: u16,
    ) -> Result<ScheduleAggregate, Error> {
        let query = vec![
            (geography_field, geography.to_owned()),
            ("cycle", cycle.to_string()),
            ("per_page", PAGE_SIZE.to_string()),
            ("sort", "-total".to_owned()),
        ];
        let rows = self.get_all(&format!("schedules/schedule_a/{endpoint}"), &query)?;
        let mut committee_ids: Vec<String> = rows
            .iter()
            .filter_map(|row| row.committee_id.clone())
            .collect();
        committee_ids.sort();
        committee_ids.dedup();

        let committees = self.committees(&committee_ids, cycle)?;
        let mut aggregate = ScheduleAggregate::default();
        for row in rows {
            let Some(id) = row.committee_id.as_deref() else {
                continue;
            };
            let Some(committee) = committees.get(id) else {
                continue;
            };
            if !committee.is_candidate_committee() {
                continue;
            }
            aggregate.total += row.total.unwrap_or_default();
            aggregate.contribution_count += row.count.unwrap_or_default();
            match committee.party.as_deref() {
                Some("DEM" | "D") => aggregate.democratic_total += row.total.unwrap_or_default(),
                Some("REP" | "R") => aggregate.republican_total += row.total.unwrap_or_default(),
                _ => {}
            }
            aggregate.committees.push(CommitteeSummary {
                committee_id: id.to_owned(),
                committee_type: committee.committee_type.clone(),
                party: committee.party.clone(),
                total: row.total.unwrap_or_default(),
                contribution_count: row.count.unwrap_or_default(),
            });
        }
        Ok(aggregate)
    }

    fn committees(&self, ids: &[String], cycle: u16) -> Result<BTreeMap<String, Committee>, Error> {
        let batches: Vec<&[String]> = ids.chunks(100).collect();
        let mut committees = BTreeMap::new();
        for window in batches.chunks(PAGE_CONCURRENCY) {
            let fetched: Vec<Result<Vec<ScheduleRow>, Error>> = std::thread::scope(|scope| {
                let handles: Vec<_> = window
                    .iter()
                    .map(|batch| {
                        scope.spawn(move || {
                            let mut query: Vec<(&str, String)> = batch
                                .iter()
                                .map(|id| ("committee_id", id.clone()))
                                .collect();
                            query.push(("cycle", cycle.to_string()));
                            query.push(("per_page", PAGE_SIZE.to_string()));
                            self.get_all("committees/", &query)
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| handle.join().unwrap_or(Err(Error::WorkerPanicked)))
                    .collect()
            });
            for rows in fetched {
                for row in rows? {
                    if let Some(committee) = row.into_committee() {
                        committees.insert(committee.committee_id.clone(), committee);
                    }
                }
            }
        }
        Ok(committees)
    }

    fn get_all(&self, endpoint: &str, query: &[(&str, String)]) -> Result<Vec<ScheduleRow>, Error> {
        let first = self.get_page(endpoint, query, 1)?;
        let pages = first
            .pagination
            .map_or(1, |pagination| pagination.pages.max(1));
        if pages > MAX_PAGES {
            return Err(Error::PaginationLimit);
        }
        let mut rows = first.results;
        let rest: Vec<u16> = (2..=pages).collect();
        for window in rest.chunks(PAGE_CONCURRENCY) {
            let fetched: Vec<Result<ApiResponse, Error>> = std::thread::scope(|scope| {
                let handles: Vec<_> = window
                    .iter()
                    .map(|page| scope.spawn(move || self.get_page(endpoint, query, *page)))
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| handle.join().unwrap_or(Err(Error::WorkerPanicked)))
                    .collect()
            });
            for page in fetched {
                rows.extend(page?.results);
            }
        }
        Ok(rows)
    }

    fn get_page(
        &self,
        endpoint: &str,
        query: &[(&str, String)],
        page: u16,
    ) -> Result<ApiResponse, Error> {
        let mut params = query.to_vec();
        params.push(("api_key", self.api_key.clone()));
        params.push(("page", page.to_string()));
        let body = self.get(endpoint, &params)?;
        serde_json::from_slice(&body).map_err(Error::Json)
    }

    fn get(&self, endpoint: &str, query: &[(&str, String)]) -> Result<Vec<u8>, Error> {
        let mut url = self
            .base_url
            .join(endpoint)
            .map_err(|error| Error::Url(error.to_string()))?;
        url.query_pairs_mut()
            .extend_pairs(query.iter().map(|(name, value)| (*name, value.as_str())));
        let mut cache_identity = url.clone();
        cache_identity.query_pairs_mut().clear().extend_pairs(
            query
                .iter()
                .filter(|(name, _)| *name != "api_key")
                .map(|(name, value)| (*name, value.as_str())),
        );
        if let Some(body) = self.read_cache(&cache_identity)? {
            return Ok(body);
        }
        if self.offline_only {
            return Err(Error::CacheMiss);
        }
        let mut attempt = 0;
        loop {
            match self.http.get(url.clone()).send() {
                Ok(response) if response.status().is_success() => {
                    let body = response.bytes().map_err(Error::Transport)?.to_vec();
                    self.write_cache(&cache_identity, &body)?;
                    return Ok(body);
                }
                Ok(response) if should_retry(response.status()) && attempt < MAX_RETRIES => {
                    let retry_after = response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.parse::<u64>().ok());
                    Self::pause(attempt, retry_after);
                    attempt += 1;
                }
                Ok(response) => return Err(Error::HttpStatus(response.status())),
                Err(error) if error.is_timeout() || error.is_connect() || error.is_request() => {
                    if attempt >= MAX_RETRIES {
                        return Err(Error::Transport(error));
                    }
                    Self::pause(attempt, None);
                    attempt += 1;
                }
                Err(error) => return Err(Error::Transport(error)),
            }
        }
    }

    fn pause(attempt: u32, retry_after: Option<u64>) {
        if let Some(seconds) = retry_after {
            std::thread::sleep(Duration::from_secs(seconds.min(60)));
            return;
        }
        let ceiling_ms = 250_u64.saturating_mul(2_u64.pow(attempt)).min(8_000);
        let delay_ms = rand::thread_rng().gen_range(0..=ceiling_ms);
        std::thread::sleep(Duration::from_millis(delay_ms));
    }

    fn read_cache(&self, url: &Url) -> Result<Option<Vec<u8>>, Error> {
        let Some(path) = self.cache_path(url) else {
            return Ok(None);
        };
        match std::fs::read(path) {
            Ok(body) => Ok(Some(body)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Error::Cache(error)),
        }
    }

    fn write_cache(&self, url: &Url, body: &[u8]) -> Result<(), Error> {
        let Some(path) = self.cache_path(url) else {
            return Ok(());
        };
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        std::fs::create_dir_all(parent).map_err(Error::Cache)?;
        let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&temporary, body).map_err(Error::Cache)?;
        std::fs::rename(&temporary, path).map_err(Error::Cache)
    }

    fn cache_path(&self, url: &Url) -> Option<PathBuf> {
        let directory = self.cache_dir.as_ref()?;
        let hash = Sha256::digest(url.as_str().as_bytes());
        let mut filename = String::with_capacity(64);
        for byte in hash {
            use std::fmt::Write as _;
            let _ = write!(filename, "{byte:02x}");
        }
        Some(directory.join(format!("{filename}.json")))
    }
}

fn should_retry(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// Aggregated candidate-committee totals for one geography and election cycle.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ScheduleAggregate {
    /// Total dollars reported by candidate committees.
    pub total: f64,
    /// Count of itemized contributions reported by candidate committees.
    pub contribution_count: u64,
    /// Total dollars reported by Democratic candidate committees.
    pub democratic_total: f64,
    /// Total dollars reported by Republican candidate committees.
    pub republican_total: f64,
    /// Candidate committee rows, identified by committee ID and classification only.
    pub committees: Vec<CommitteeSummary>,
}

/// Committee classification and its aggregated Schedule A values.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct CommitteeSummary {
    /// Public FEC committee identifier.
    pub committee_id: String,
    /// FEC committee type, such as `H`, `S`, or `P`.
    pub committee_type: Option<String>,
    /// FEC party code, when available.
    pub party: Option<String>,
    /// Aggregate dollars for this committee in the requested geography and cycle.
    pub total: f64,
    /// Aggregate contribution count for this committee.
    pub contribution_count: u64,
}

/// Public committee metadata required to classify aggregate schedule rows.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Committee {
    /// Public FEC committee identifier.
    pub committee_id: String,
    /// FEC committee type, such as `H`, `S`, or `P`.
    pub committee_type: Option<String>,
    /// FEC party code, when available.
    pub party: Option<String>,
}

impl Committee {
    /// Returns whether this record identifies a House, Senate, or presidential committee.
    #[must_use]
    pub fn is_candidate_committee(&self) -> bool {
        matches!(self.committee_type.as_deref(), Some("H" | "S" | "P"))
    }
}

/// Errors returned by the `OpenFEC` client.
#[derive(Debug)]
pub enum Error {
    /// No non-empty API key was supplied.
    MissingKey,
    /// Offline mode could not find a matching cached response.
    CacheMiss,
    /// The HTTP request or response body failed.
    Transport(reqwest::Error),
    /// The API returned a non-retryable HTTP status.
    HttpStatus(StatusCode),
    /// The response body did not match the `OpenFEC` JSON format.
    Json(serde_json::Error),
    /// URL parsing or joining failed.
    Url(String),
    /// Local cache access failed.
    Cache(std::io::Error),
    /// Pagination exceeded the supported maximum.
    PaginationLimit,
    /// A page-fetch worker thread panicked.
    WorkerPanicked,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingKey => write!(formatter, "missing {API_KEY_ENV}"),
            Self::CacheMiss => write!(formatter, "OpenFEC cache miss in offline mode"),
            Self::Transport(error) => write!(formatter, "OpenFEC transport error: {error}"),
            Self::HttpStatus(status) => write!(formatter, "OpenFEC returned HTTP {status}"),
            Self::Json(error) => write!(formatter, "invalid OpenFEC JSON: {error}"),
            Self::Url(error) => write!(formatter, "invalid OpenFEC URL: {error}"),
            Self::Cache(error) => write!(formatter, "OpenFEC cache error: {error}"),
            Self::PaginationLimit => write!(formatter, "OpenFEC pagination exceeded 500 pages"),
            Self::WorkerPanicked => write!(formatter, "OpenFEC page worker panicked"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Cache(error) => Some(error),
            Self::MissingKey
            | Self::CacheMiss
            | Self::Url(_)
            | Self::HttpStatus(_)
            | Self::PaginationLimit
            | Self::WorkerPanicked => None,
        }
    }
}

#[derive(Deserialize)]
struct ApiResponse {
    #[serde(default)]
    results: Vec<ScheduleRow>,
    pagination: Option<Pagination>,
}

#[derive(Deserialize)]
struct Pagination {
    pages: u16,
}

#[derive(Deserialize)]
struct ScheduleRow {
    committee_id: Option<String>,
    #[serde(default)]
    total: Option<f64>,
    #[serde(default)]
    count: Option<u64>,
    committee_type: Option<String>,
    party: Option<String>,
}

impl ScheduleRow {
    fn into_committee(self) -> Option<Committee> {
        Some(Committee {
            committee_id: self.committee_id?,
            committee_type: self.committee_type,
            party: self.party.map(|party| party.to_ascii_uppercase()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_synthetic_openfec_layout_without_contributor_fields() {
        let document: ApiResponse =
            serde_json::from_str(include_str!("../tests/fixtures/schedule-a-by-zip.json"))
                .expect("synthetic fixture uses the OpenFEC response layout");
        assert_eq!(document.pagination.as_ref().map(|p| p.pages), Some(1));
        assert_eq!(document.results.len(), 2);
        assert_eq!(
            document.results[0].committee_id.as_deref(),
            Some("C99990001")
        );
        assert_eq!(document.results[0].total, Some(123.45));
        assert_eq!(document.results[0].count, Some(3));

        let committee_document: ApiResponse =
            serde_json::from_str(include_str!("../tests/fixtures/committees.json"))
                .expect("synthetic committee fixture uses the OpenFEC response layout");
        let committee = committee_document
            .results
            .into_iter()
            .next()
            .expect("committee fixture has a result")
            .into_committee()
            .expect("committee fixture has a committee ID");
        assert_eq!(committee.committee_id, "C99990001");
        assert_eq!(committee.committee_type.as_deref(), Some("H"));
        assert_eq!(committee.party.as_deref(), Some("DEM"));
    }

    #[test]
    fn committee_classification_includes_candidate_types_only() {
        let house = Committee {
            committee_id: "C99990001".to_owned(),
            committee_type: Some("H".to_owned()),
            party: Some("DEM".to_owned()),
        };
        let pac = Committee {
            committee_id: "C99990002".to_owned(),
            committee_type: Some("N".to_owned()),
            party: None,
        };
        assert!(house.is_candidate_committee());
        assert!(!pac.is_candidate_committee());
    }
}
