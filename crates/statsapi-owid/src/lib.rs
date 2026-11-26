//! Streaming client for Our World in Data (OWID) grapher CSV endpoints.
//!
//! A chart is read from `https://ourworldindata.org/grapher/<slug>.csv`. The
//! response is filtered to one country while it streams; nothing is written to
//! disk, so no cache or bulk archive is ever kept.

use reqwest::blocking::Client as HttpClient;
use reqwest::Url;
use std::fmt;
use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BASE: &str = "https://ourworldindata.org/grapher";
/// Upper bound on bytes read from one response, so a runaway body cannot fill memory or the network.
const MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;
/// Column of the grapher CSV that holds the charted value (after Entity, Code, Year).
const VALUE_COLUMN: usize = 3;

/// How the country's rows were obtained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Path {
    /// The server already restricted the response to the requested country.
    Api,
    /// The server returned other countries too; rows were filtered while streaming.
    Stream,
}

impl Path {
    /// Short label recorded in `measurement.source`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Stream => "stream",
        }
    }
}

/// The newest numeric observation of one chart for one country.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    /// Entity name as published, such as `United States`.
    pub entity: String,
    /// ISO 3166-1 alpha-3 code.
    pub code: String,
    /// Year of the observation.
    pub year: i32,
    /// Charted value.
    pub value: f64,
    /// Header of the charted column.
    pub column: String,
    /// Exact URL requested.
    pub url: String,
    /// Whether the server or the stream did the country filtering.
    pub path: Path,
}

/// Errors returned while building or reading an OWID request.
#[derive(Debug)]
#[non_exhaustive]
pub enum ClientError {
    /// The slug or country code is malformed.
    InvalidQuery(String),
    /// An HTTP request failed after retries.
    Http(reqwest::Error),
    /// The server answered with a non-success status.
    Status(u16),
    /// The body was not readable CSV.
    Csv(csv::Error),
    /// The body had no row with a numeric value for the country.
    NoObservation,
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQuery(message) => write!(f, "invalid query: {message}"),
            Self::Http(error) => write!(f, "OWID request failed: {error}"),
            Self::Status(status) => write!(f, "OWID returned HTTP {status}"),
            Self::Csv(error) => write!(f, "invalid OWID CSV: {error}"),
            Self::NoObservation => write!(f, "no numeric OWID observation for the country"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Csv(error) => Some(error),
            Self::InvalidQuery(_) | Self::Status(_) | Self::NoObservation => None,
        }
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<csv::Error> for ClientError {
    fn from(error: csv::Error) -> Self {
        Self::Csv(error)
    }
}

/// A blocking OWID grapher client.
#[derive(Clone, Debug)]
pub struct OwidClient {
    http: HttpClient,
    max_retries: usize,
}

impl OwidClient {
    /// Creates a client with a 60 second timeout and three retries.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be initialized.
    pub fn new() -> Result<Self, ClientError> {
        Ok(Self {
            http: HttpClient::builder()
                .user_agent(concat!("statsapi-owid/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(60))
                .build()?,
            max_retries: 3,
        })
    }

    /// Sets the number of retries after the initial attempt.
    #[must_use]
    pub fn with_max_retries(mut self, retries: usize) -> Self {
        self.max_retries = retries;
        self
    }

    /// Builds the request URL for a grapher slug and country.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::InvalidQuery`] for a slug outside `[a-z0-9-]` or a
    /// country that is not an uppercase ISO alpha-3 code.
    pub fn url(slug: &str, country: &str) -> Result<String, ClientError> {
        if slug.is_empty()
            || !slug
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(ClientError::InvalidQuery("invalid grapher slug".into()));
        }
        validate_country(country)?;
        let mut url = Url::parse(&format!("{BASE}/{slug}.csv"))
            .map_err(|error| ClientError::InvalidQuery(error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("csvType", "filtered")
            .append_pair("country", &format!("~{country}"));
        Ok(url.into())
    }

    /// Streams the chart CSV and returns the country's newest numeric value.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid input, network or HTTP failures, malformed
    /// CSV, or when the country has no numeric value.
    pub fn latest(&self, slug: &str, country: &str) -> Result<Observation, ClientError> {
        let url = Self::url(slug, country)?;
        let response = self.get_with_retry(&url)?;
        latest_in_csv(response.take(MAX_RESPONSE_BYTES), country, &url)
    }

    fn get_with_retry(&self, url: &str) -> Result<reqwest::blocking::Response, ClientError> {
        let mut attempt = 0;
        loop {
            let outcome = self.http.get(url).send();
            let retry = match &outcome {
                Ok(response) if response.status().is_success() => return Ok(outcome?),
                Ok(response) => retryable_status(response.status().as_u16()),
                Err(_) => true,
            };
            if !retry || attempt >= self.max_retries {
                return match outcome {
                    Ok(response) => Err(ClientError::Status(response.status().as_u16())),
                    Err(error) => Err(ClientError::Http(error)),
                };
            }
            jittered_delay(attempt);
            attempt += 1;
        }
    }
}

/// Reads grapher CSV from `reader` and returns the newest numeric row for `country`.
///
/// The value is the first column after `Entity`, `Code` and `Year`. Rows of
/// other countries are skipped as they stream past.
///
/// # Errors
///
/// Returns [`ClientError::Csv`] for unreadable CSV and
/// [`ClientError::NoObservation`] when the country has no numeric value.
pub fn latest_in_csv(
    reader: impl Read,
    country: &str,
    url: &str,
) -> Result<Observation, ClientError> {
    let mut csv = csv::ReaderBuilder::new().flexible(true).from_reader(reader);
    let column = csv
        .headers()?
        .get(VALUE_COLUMN)
        .ok_or(ClientError::NoObservation)?
        .to_string();
    let mut other_countries = false;
    let mut best: Option<Observation> = None;
    for row in csv.records() {
        let row = row?;
        if row.get(1) != Some(country) {
            other_countries = true;
            continue;
        }
        let year = row.get(2).and_then(|year| year.parse::<i32>().ok());
        let value = row
            .get(VALUE_COLUMN)
            .and_then(|value| value.parse::<f64>().ok());
        let (Some(year), Some(value)) = (year, value) else {
            continue;
        };
        if best.as_ref().is_none_or(|current| year > current.year) {
            best = Some(Observation {
                entity: row.get(0).unwrap_or_default().to_string(),
                code: country.to_string(),
                year,
                value,
                column: column.clone(),
                url: url.to_string(),
                path: Path::Api,
            });
        }
    }
    let mut observation = best.ok_or(ClientError::NoObservation)?;
    if other_countries {
        observation.path = Path::Stream;
    }
    Ok(observation)
}

fn validate_country(country: &str) -> Result<(), ClientError> {
    if country.len() != 3 || !country.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(ClientError::InvalidQuery(
            "country must be ISO alpha-3".into(),
        ));
    }
    Ok(())
}

fn retryable_status(status: u16) -> bool {
    status == 429 || (500..=599).contains(&status)
}

fn jittered_delay(attempt: usize) {
    let base_ms = 200_u64.saturating_mul(1_u64 << attempt.min(5));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let delay = base_ms / 2 + u64::from(nanos) % base_ms;
    std::thread::sleep(Duration::from_millis(delay));
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILTERED: &str = include_str!("../tests/fixtures/filtered.csv");
    const UNFILTERED: &str = include_str!("../tests/fixtures/unfiltered.csv");

    #[test]
    fn server_filtered_body_is_the_api_path_and_takes_the_newest_year() {
        let got = latest_in_csv(FILTERED.as_bytes(), "USA", "u").expect("observation");
        assert_eq!((got.year, got.value), (2023, 79.5));
        assert_eq!(got.path, Path::Api);
        assert_eq!(got.column, "Life expectancy");
    }

    #[test]
    fn unfiltered_body_is_streamed_and_skips_empty_values() {
        let got = latest_in_csv(UNFILTERED.as_bytes(), "USA", "u").expect("observation");
        assert_eq!((got.year, got.value), (2024, 0.735));
        assert_eq!(got.path, Path::Stream);
        assert_eq!(got.entity, "United States");
    }

    #[test]
    fn country_without_numeric_rows_is_no_observation() {
        let got = latest_in_csv(UNFILTERED.as_bytes(), "FRA", "u");
        assert!(matches!(got, Err(ClientError::NoObservation)), "{got:?}");
    }

    #[test]
    fn url_is_validated_and_filters_one_country() {
        let url = OwidClient::url("life-expectancy", "USA").expect("url");
        assert_eq!(
            url,
            "https://ourworldindata.org/grapher/life-expectancy.csv?csvType=filtered&country=%7EUSA"
        );
        assert!(OwidClient::url("../x", "USA").is_err());
        assert!(OwidClient::url("life-expectancy", "us").is_err());
    }

    #[test]
    #[ignore = "live Our World in Data smoke test"]
    fn live_latest_reads_us_life_expectancy() {
        let got = OwidClient::new()
            .expect("client")
            .latest("life-expectancy", "USA")
            .expect("observation");
        assert!(got.value > 60.0 && got.year >= 2020, "{got:?}");
    }
}
