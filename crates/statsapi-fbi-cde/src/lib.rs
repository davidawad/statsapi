//! Blocking client for the FBI Crime Data Explorer (CDE) summarized-offense API.
//!
//! The client always queries the live API; it keeps no cache. Monthly offense
//! rates (per 100,000 people) and counts are returned as published, with
//! helpers to roll them up into calendar years.

use rand::Rng;
use reqwest::blocking::Client as HttpClient;
use reqwest::{StatusCode, Url};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

/// CDE base URL (served through api.data.gov).
pub const API_BASE: &str = "https://api.usa.gov/crime/fbi/cde/";
/// Environment variable read by [`Client::from_env`].
pub const API_KEY_ENV: &str = "API_DATA_GOV_KEY";
const MAX_RETRIES: u32 = 4;

/// Monthly values keyed by the CDE `MM-YYYY` label.
pub type Monthly = BTreeMap<String, f64>;

/// Offense groups published by the summarized endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offense {
    /// Violent crime (murder, rape, robbery, aggravated assault).
    Violent,
    /// Property crime (burglary, larceny-theft, motor vehicle theft, arson).
    Property,
}

impl Offense {
    /// Path segment the CDE API uses for this offense group.
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Violent => "violent-crime",
            Self::Property => "property-crime",
        }
    }
}

/// One agency row from the by-state agency listing.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Agency {
    /// Originating agency identifier (ORI).
    pub ori: String,
    /// County name(s) as published (upper case; several are `;`-separated).
    #[serde(default)]
    pub counties: String,
    /// Agency display name; also the series name inside summaries.
    pub agency_name: String,
    /// State postal abbreviation.
    #[serde(default)]
    pub state_abbr: String,
    /// State name, as used by the state series in summaries.
    #[serde(default)]
    pub state_name: String,
}

impl Agency {
    /// Returns whether the agency covers `county` (case-insensitive, without a "County" suffix).
    #[must_use]
    pub fn in_county(&self, county: &str) -> bool {
        let wanted = normalize_county(county);
        self.counties
            .split([';', ','])
            .any(|part| normalize_county(part) == wanted)
    }
}

/// Upper-cases a county name and drops a trailing "County"/"Parish".
#[must_use]
pub fn normalize_county(name: &str) -> String {
    let upper = name.trim().to_ascii_uppercase();
    for suffix in [" COUNTY", " PARISH"] {
        if let Some(stripped) = upper.strip_suffix(suffix) {
            return stripped.trim().to_owned();
        }
    }
    upper
}

/// Decoded summarized-offense response.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    /// Monthly offense rates per 100,000, keyed by series name (without the " Offenses" suffix).
    pub rates: BTreeMap<String, Monthly>,
    /// Monthly offense counts by series name, when the endpoint publishes them.
    pub actuals: BTreeMap<String, Monthly>,
    /// Monthly population by series name.
    pub population: BTreeMap<String, Monthly>,
}

impl Summary {
    /// Decodes a CDE summarized response body.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Json`] when the body is not the CDE summarized layout.
    pub fn from_slice(body: &[u8]) -> Result<Self, Error> {
        let raw: RawSummary = serde_json::from_slice(body).map_err(Error::Json)?;
        let offenses = raw.offenses.unwrap_or_default();
        Ok(Self {
            rates: strip_suffix(offenses.rates.unwrap_or_default()),
            actuals: strip_suffix(offenses.actuals.unwrap_or_default()),
            population: raw
                .populations
                .and_then(|populations| populations.population)
                .unwrap_or_default(),
        })
    }

    /// The jurisdiction's own rate series: the one that is neither the nation nor in `exclude`.
    #[must_use]
    pub fn series_other_than<'a>(&'a self, exclude: &[&str]) -> Option<(&'a str, &'a Monthly)> {
        self.rates
            .iter()
            .find(|(name, _)| name.as_str() != "United States" && !exclude.contains(&name.as_str()))
            .map(|(name, monthly)| (name.as_str(), monthly))
    }
}

/// Sum of the twelve months of `year`, or `None` unless all twelve are present.
#[must_use]
pub fn year_sum(monthly: &Monthly, year: i32) -> Option<f64> {
    let months = year_months(monthly, year);
    (months.len() == 12).then(|| months.iter().sum())
}

/// Mean of the months of `year` present in `monthly`, or `None` when none are.
#[must_use]
pub fn year_mean(monthly: &Monthly, year: i32) -> Option<f64> {
    let months = year_months(monthly, year);
    #[expect(clippy::cast_precision_loss, reason = "at most twelve months")]
    (!months.is_empty()).then(|| months.iter().sum::<f64>() / months.len() as f64)
}

fn year_months(monthly: &Monthly, year: i32) -> Vec<f64> {
    let suffix = format!("-{year}");
    monthly
        .iter()
        .filter(|(label, _)| label.ends_with(&suffix))
        .map(|(_, value)| *value)
        .collect()
}

fn strip_suffix(
    series: BTreeMap<String, BTreeMap<String, Option<f64>>>,
) -> BTreeMap<String, Monthly> {
    series
        .into_iter()
        .filter_map(|(name, monthly)| {
            let name = name.strip_suffix(" Offenses")?.to_owned();
            let monthly = monthly
                .into_iter()
                .filter_map(|(label, value)| Some((label, value?)))
                .collect();
            Some((name, monthly))
        })
        .collect()
}

/// Blocking client for the CDE API.
pub struct Client {
    api_key: String,
    http: HttpClient,
    base_url: Url,
}

impl fmt::Debug for Client {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Client")
            .field("api_key", &"[REDACTED]")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Creates a client with a caller-supplied api.data.gov key.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`] for an empty key or a transport error if
    /// the HTTP client cannot be built.
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
            api_key: api_key.trim().to_owned(),
            http,
            base_url,
        })
    }

    /// Creates a client from `API_DATA_GOV_KEY`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`] when the variable is absent or empty.
    pub fn from_env() -> Result<Self, Error> {
        Self::new(std::env::var(API_KEY_ENV).map_err(|_| Error::MissingKey)?)
    }

    /// Points the client at another base URL (a test server or mirror).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Url`] when `base` is not a URL.
    pub fn with_base_url(mut self, base: &str) -> Result<Self, Error> {
        let mut base = base.to_owned();
        if !base.ends_with('/') {
            base.push('/');
        }
        self.base_url = Url::parse(&base).map_err(|error| Error::Url(error.to_string()))?;
        Ok(self)
    }

    /// Public URL of the state summary endpoint (no key).
    #[must_use]
    pub fn state_url(&self, state_abbr: &str, offense: Offense, from: i32, to: i32) -> String {
        format!(
            "{}summarized/state/{state_abbr}/{}?from=01-{from}&to=12-{to}",
            self.base_url,
            offense.slug()
        )
    }

    /// Public URL of the agency summary endpoint (no key).
    #[must_use]
    pub fn agency_url(&self, ori: &str, offense: Offense, from: i32, to: i32) -> String {
        format!(
            "{}summarized/agency/{ori}/{}?from=01-{from}&to=12-{to}",
            self.base_url,
            offense.slug()
        )
    }

    /// Lists every agency of a state, flattened from the by-county grouping.
    ///
    /// # Errors
    ///
    /// Returns an error for failed requests or an unexpected layout.
    pub fn agencies_by_state(&self, state_abbr: &str) -> Result<Vec<Agency>, Error> {
        let body = self.get(&format!("{}agency/byStateAbbr/{state_abbr}", self.base_url))?;
        let grouped: BTreeMap<String, Vec<Agency>> =
            serde_json::from_slice(&body).map_err(Error::Json)?;
        Ok(grouped.into_values().flatten().collect())
    }

    /// Monthly state offense rates for calendar years `from..=to`, with the national series.
    ///
    /// # Errors
    ///
    /// Returns an error for failed requests or an unexpected layout.
    pub fn state_summary(
        &self,
        state_abbr: &str,
        offense: Offense,
        from: i32,
        to: i32,
    ) -> Result<Summary, Error> {
        Summary::from_slice(&self.get(&self.state_url(state_abbr, offense, from, to))?)
    }

    /// Monthly agency offense rates, counts and population for calendar years `from..=to`.
    ///
    /// # Errors
    ///
    /// Returns an error for failed requests or an unexpected layout.
    pub fn agency_summary(
        &self,
        ori: &str,
        offense: Offense,
        from: i32,
        to: i32,
    ) -> Result<Summary, Error> {
        Summary::from_slice(&self.get(&self.agency_url(ori, offense, from, to))?)
    }

    fn get(&self, url: &str) -> Result<Vec<u8>, Error> {
        let mut url = Url::parse(url).map_err(|error| Error::Url(error.to_string()))?;
        url.query_pairs_mut().append_pair("API_KEY", &self.api_key);
        let mut attempt = 0;
        loop {
            match self.http.get(url.clone()).send() {
                Ok(response) if response.status().is_success() => {
                    return response
                        .bytes()
                        .map(|body| body.to_vec())
                        .map_err(Error::Transport);
                }
                Ok(response) if should_retry(response.status()) && attempt < MAX_RETRIES => {
                    let retry_after = response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.parse::<u64>().ok());
                    pause(attempt, retry_after);
                    attempt += 1;
                }
                Ok(response) => return Err(Error::HttpStatus(response.status())),
                Err(error) if error.is_timeout() || error.is_connect() || error.is_request() => {
                    if attempt >= MAX_RETRIES {
                        return Err(Error::Transport(error));
                    }
                    pause(attempt, None);
                    attempt += 1;
                }
                Err(error) => return Err(Error::Transport(error)),
            }
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

fn should_retry(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// Errors returned by the CDE client.
#[derive(Debug)]
pub enum Error {
    /// No non-empty API key was supplied.
    MissingKey,
    /// The HTTP request or response body failed.
    Transport(reqwest::Error),
    /// The API returned a non-retryable (or exhausted-retry) HTTP status.
    HttpStatus(StatusCode),
    /// The response body did not match the CDE layout.
    Json(serde_json::Error),
    /// URL parsing failed.
    Url(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingKey => write!(formatter, "missing {API_KEY_ENV}"),
            Self::Transport(error) => write!(formatter, "FBI CDE transport error: {error}"),
            Self::HttpStatus(status) => write!(formatter, "FBI CDE returned HTTP {status}"),
            Self::Json(error) => write!(formatter, "invalid FBI CDE JSON: {error}"),
            Self::Url(error) => write!(formatter, "invalid FBI CDE URL: {error}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::MissingKey | Self::HttpStatus(_) | Self::Url(_) => None,
        }
    }
}

#[derive(Deserialize)]
struct RawSummary {
    offenses: Option<RawOffenses>,
    populations: Option<RawPopulations>,
}

#[derive(Default, Deserialize)]
struct RawOffenses {
    rates: Option<BTreeMap<String, BTreeMap<String, Option<f64>>>>,
    actuals: Option<BTreeMap<String, BTreeMap<String, Option<f64>>>>,
}

#[derive(Deserialize)]
struct RawPopulations {
    population: Option<BTreeMap<String, Monthly>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Summary {
        Summary::from_slice(include_bytes!("../tests/fixtures/agency-summary.json"))
            .expect("fixture decodes")
    }

    #[test]
    fn decodes_agency_layout_and_strips_offense_suffix() {
        let summary = fixture();
        assert!(summary
            .rates
            .contains_key("Example Township Police Department"));
        assert!(summary.rates.contains_key("United States"));
        assert!(!summary
            .actuals
            .contains_key("Example Township Police Department Clearances"));
        let (name, _) = summary
            .series_other_than(&["Example State"])
            .expect("agency series");
        assert_eq!(name, "Example Township Police Department");
    }

    #[test]
    fn year_helpers_require_a_full_year_for_sums() {
        let summary = fixture();
        let counts = &summary.actuals["Example Township Police Department"];
        assert_eq!(year_sum(counts, 2023), Some(78.0));
        assert_eq!(year_sum(counts, 2024), None);
        let population = &summary.population["Example Township Police Department"];
        assert_eq!(year_mean(population, 2023), Some(50_000.0));
        assert_eq!(year_mean(population, 2019), None);
    }

    #[test]
    fn county_matching_ignores_case_and_suffix() {
        let agency = Agency {
            ori: "XX0000100".into(),
            counties: "SAINT JOHNS; ALPHA".into(),
            agency_name: "A".into(),
            state_abbr: "XX".into(),
            state_name: "Example State".into(),
        };
        assert!(agency.in_county("Alpha County"));
        assert!(agency.in_county("saint johns"));
        assert!(!agency.in_county("Beta"));
    }

    #[test]
    fn agency_listing_flattens_county_groups() {
        let grouped: BTreeMap<String, Vec<Agency>> =
            serde_json::from_slice(include_bytes!("../tests/fixtures/agencies.json"))
                .expect("fixture decodes");
        let all: Vec<Agency> = grouped.into_values().flatten().collect();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn urls_omit_the_key() {
        let client = Client::new("secret").expect("client");
        let url = client.state_url("XX", Offense::Violent, 2019, 2024);
        assert!(url.ends_with("summarized/state/XX/violent-crime?from=01-2019&to=12-2024"));
        assert!(!format!("{client:?}").contains("secret"));
    }

    #[test]
    fn empty_key_is_rejected() {
        assert!(matches!(Client::new("  "), Err(Error::MissingKey)));
    }
}
