//! Blocking client for two annual governance indices: the Transparency
//! International Corruption Perceptions Index (CPI) and the Reporters Without
//! Borders (RSF) World Press Freedom Index.
//!
//! Each edition is streamed into memory, parsed, and dropped with the
//! [`Edition`]; nothing is written to disk. The newest published edition is
//! found by probing the publisher's per-year URL from the current year down.

mod cpi;
mod rsf;

use reqwest::blocking::Client as HttpClient;
use std::fmt;
use std::time::Duration;

const CPI_BASE: &str = "https://images.transparencycdn.org/images";
const RSF_BASE: &str = "https://rsf.org/sites/default/files/import_classement";
const USER_AGENT: &str = "statsapi-governance-indices/0.1 (keyless annual reference lookup)";
const MAX_BODY_BYTES: usize = 25 * 1024 * 1024;
const PROBE_YEARS: u16 = 3;

/// A published annual index.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Index {
    /// Transparency International Corruption Perceptions Index (0-100, higher is cleaner).
    Cpi,
    /// RSF World Press Freedom Index (0-100, higher is freer).
    Rsf,
}

impl Index {
    /// Short stable name used in definitions and in `source_url` provenance.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Cpi => "cpi",
            Self::Rsf => "rsf",
        }
    }

    /// Parses [`Index::name`].
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "cpi" => Some(Self::Cpi),
            "rsf" => Some(Self::Rsf),
            _ => None,
        }
    }
}

/// One country's score and rank in one edition, with the prior edition's values.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    /// ISO 3166-1 alpha-3 country code.
    pub iso3: String,
    /// Score on the index's 0-100 scale.
    pub score: f64,
    /// Rank, 1 being best.
    pub rank: Option<u32>,
    /// Score in the previous edition.
    pub prior_score: Option<f64>,
    /// Rank in the previous edition.
    pub prior_rank: Option<u32>,
}

impl Observation {
    /// Score change versus the prior edition, rounded to two decimals.
    #[must_use]
    pub fn score_change(&self) -> Option<f64> {
        let delta = self.score - self.prior_score?;
        Some((delta * 100.0).round() / 100.0)
    }

    /// Places gained versus the prior edition (positive means moved up).
    #[must_use]
    pub fn rank_change(&self) -> Option<i64> {
        Some(i64::from(self.prior_rank?) - i64::from(self.rank?))
    }
}

/// One parsed edition of an index.
#[derive(Clone, Debug, PartialEq)]
pub struct Edition {
    /// Which index this is.
    pub index: Index,
    /// Edition year.
    pub year: u16,
    /// URL the edition was streamed from.
    pub url: String,
    /// All country rows.
    pub observations: Vec<Observation>,
}

impl Edition {
    /// The row for an ISO 3166-1 alpha-3 code, if the index covers it.
    #[must_use]
    pub fn country(&self, iso3: &str) -> Option<&Observation> {
        self.observations
            .iter()
            .find(|row| row.iso3.eq_ignore_ascii_case(iso3))
    }
}

/// Errors from fetching or parsing an edition.
#[derive(Debug)]
#[non_exhaustive]
pub enum ClientError {
    /// The HTTP request failed.
    Http(reqwest::Error),
    /// The publisher has no edition at this URL (non-success status).
    NotPublished {
        /// Probed URL.
        url: String,
        /// HTTP status code.
        status: u16,
    },
    /// The body was larger than the safety cap.
    TooLarge(String),
    /// The body did not have the expected layout.
    Parse(String),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(error) => write!(f, "request failed: {error}"),
            Self::NotPublished { url, status } => write!(f, "HTTP {status} from {url}"),
            Self::TooLarge(url) => write!(f, "response from {url} exceeds the size cap"),
            Self::Parse(message) => write!(f, "unexpected layout: {message}"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

/// Publisher URL of one edition.
#[must_use]
pub fn edition_url(index: Index, year: u16) -> String {
    match index {
        Index::Cpi => format!("{CPI_BASE}/CPI{year}_Results.xlsx"),
        Index::Rsf => format!("{RSF_BASE}/{year}.csv"),
    }
}

/// Parses a CPI results workbook (the "CPI Timeseries" sheet) into an edition.
///
/// # Errors
///
/// Returns [`ClientError::Parse`] when the workbook or sheet layout is not recognised.
pub fn parse_cpi_xlsx(bytes: &[u8], url: &str) -> Result<Edition, ClientError> {
    cpi::parse(bytes, url)
}

/// Parses an RSF index CSV for the edition `year`.
///
/// # Errors
///
/// Returns [`ClientError::Parse`] when the header is not recognised.
pub fn parse_rsf_csv(bytes: &[u8], year: u16, url: &str) -> Result<Edition, ClientError> {
    rsf::parse(bytes, year, url)
}

/// A blocking client that streams index editions without keeping them.
#[derive(Clone, Debug)]
pub struct GovernanceClient {
    http: HttpClient,
}

impl GovernanceClient {
    /// Builds a client with a 60 second timeout.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::Http`] when the HTTP stack cannot be built.
    pub fn new() -> Result<Self, ClientError> {
        let http = HttpClient::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self { http })
    }

    /// Streams and parses the edition for `year`.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::NotPublished`] when the publisher has no such
    /// edition, or another variant when the request or parse fails.
    pub fn fetch_year(&self, index: Index, year: u16) -> Result<Edition, ClientError> {
        let url = edition_url(index, year);
        let response = match self.http.get(&url).send() {
            Ok(response) => response,
            Err(_) => self.http.get(&url).send()?,
        };
        let status = response.status();
        if !status.is_success() {
            return Err(ClientError::NotPublished {
                url,
                status: status.as_u16(),
            });
        }
        let body = response.bytes()?;
        if body.len() > MAX_BODY_BYTES {
            return Err(ClientError::TooLarge(url));
        }
        match index {
            Index::Cpi => parse_cpi_xlsx(&body, &url),
            Index::Rsf => parse_rsf_csv(&body, year, &url),
        }
    }

    /// Streams the newest edition published in `current_year` or the two years before.
    ///
    /// # Errors
    ///
    /// Returns the last probe error when no edition is found.
    pub fn latest(&self, index: Index, current_year: u16) -> Result<Edition, ClientError> {
        let mut last = None;
        for year in (current_year.saturating_sub(PROBE_YEARS - 1)..=current_year).rev() {
            match self.fetch_year(index, year) {
                Ok(edition) => return Ok(edition),
                Err(error) => last = Some(error),
            }
        }
        Err(last.unwrap_or_else(|| ClientError::Parse("no years probed".into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CPI_FIXTURE: &[u8] = include_bytes!("../tests/fixtures/cpi-results.xlsx");
    const RSF_FIXTURE: &[u8] = include_bytes!("../tests/fixtures/rsf-2026.csv");

    #[test]
    fn cpi_workbook_yields_newest_edition_with_prior_values() {
        let edition = parse_cpi_xlsx(CPI_FIXTURE, "u").expect("fixture parses");
        assert_eq!((edition.index, edition.year), (Index::Cpi, 2025));
        let den = edition.country("DNK").expect("Denmark");
        assert_eq!((den.score, den.rank), (89.0, Some(1)));
        let afg = edition.country("afg").expect("Afghanistan");
        assert_eq!((afg.score, afg.rank), (16.0, Some(169)));
        assert_eq!((afg.prior_score, afg.prior_rank), (Some(17.0), Some(165)));
        assert_eq!(afg.score_change(), Some(-1.0));
        assert_eq!(afg.rank_change(), Some(-4));
        assert!(edition.country("ZZZ").is_none());
    }

    #[test]
    fn rsf_csv_uses_decimal_commas_and_prior_columns() {
        let edition = parse_rsf_csv(RSF_FIXTURE, 2026, "u").expect("fixture parses");
        assert_eq!((edition.index, edition.year), (Index::Rsf, 2026));
        let usa = edition.country("USA").expect("United States");
        assert!((usa.score - 62.61).abs() < 1e-9);
        assert_eq!((usa.rank, usa.prior_rank), (Some(64), Some(57)));
        assert_eq!(usa.score_change(), Some(-2.88));
        assert_eq!(usa.rank_change(), Some(-7));
    }

    #[test]
    fn unrecognised_bodies_are_parse_errors() {
        assert!(matches!(
            parse_rsf_csv(b"<html>404</html>", 2027, "u"),
            Err(ClientError::Parse(_))
        ));
        assert!(matches!(
            parse_cpi_xlsx(b"not a zip", "u"),
            Err(ClientError::Parse(_))
        ));
    }

    #[test]
    fn urls_and_names_round_trip() {
        assert_eq!(
            edition_url(Index::Cpi, 2025),
            "https://images.transparencycdn.org/images/CPI2025_Results.xlsx"
        );
        assert_eq!(Index::from_name(Index::Rsf.name()), Some(Index::Rsf));
        assert_eq!(Index::from_name("gpi"), None);
    }

    #[test]
    #[ignore = "live: streams the newest CPI and RSF editions"]
    fn live_newest_editions_cover_the_united_states() {
        let client = GovernanceClient::new().expect("client");
        for index in [Index::Cpi, Index::Rsf] {
            let edition = client.latest(index, 2026).expect("live edition");
            let usa = edition.country("USA").expect("USA row");
            println!(
                "{} {} USA score={} rank={:?} prior_score={:?} prior_rank={:?} {}",
                index.name(),
                edition.year,
                usa.score,
                usa.rank,
                usa.prior_score,
                usa.prior_rank,
                edition.url
            );
            assert!(usa.score > 0.0 && usa.score <= 100.0);
            assert!(edition.observations.len() > 100);
        }
    }
}
