//! Streaming client for the UN World Population Prospects (WPP) CSV downloads.
//!
//! The UN Population Division Data Portal API requires a bearer token, so this
//! client reads the keyless, gzip-compressed "Demographic Indicators, Medium
//! variant" CSV. The body is decoded and filtered on the fly and nothing is
//! written to disk: only the rows of the requested country are kept.

use flate2::read::MultiGzDecoder;
use reqwest::blocking::Client as HttpClient;
use std::collections::BTreeMap;
use std::fmt;
use std::io::Read;
use std::time::Duration;

/// Medium-variant demographic indicators, WPP 2024 revision.
pub const MEDIUM_URL: &str = "https://population.un.org/wpp/assets/Excel%20Files/1_Indicator%20(Standard)/CSV_FILES/WPP2024_Demographic_Indicators_Medium.csv.gz";
/// Revision label carried in every record's vintage.
pub const REVISION: &str = "WPP2024";
/// Last year of the WPP 2024 estimates; later years are medium projections.
pub const LAST_ESTIMATE_YEAR: i32 = 2023;

/// Errors returned while streaming or parsing a WPP download.
#[derive(Debug)]
#[non_exhaustive]
pub enum ClientError {
    /// The country code is not an uppercase ISO alpha-3 code.
    InvalidCountry(String),
    /// An HTTP request failed after retries.
    Http(reqwest::Error),
    /// The CSV body could not be decoded.
    Csv(csv::Error),
    /// The CSV header lacks a required column.
    MissingColumn(&'static str),
    /// The gzip stream was unreadable.
    Io(std::io::Error),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCountry(code) => write!(f, "country must be ISO alpha-3, got {code:?}"),
            Self::Http(error) => write!(f, "UN WPP request failed: {error}"),
            Self::Csv(error) => write!(f, "invalid UN WPP CSV: {error}"),
            Self::MissingColumn(name) => write!(f, "UN WPP CSV lacks column {name}"),
            Self::Io(error) => write!(f, "UN WPP stream failed: {error}"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Csv(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::InvalidCountry(_) | Self::MissingColumn(_) => None,
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

impl From<std::io::Error> for ClientError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// One country-year row; `values` maps a WPP column name to its number.
#[derive(Clone, Debug, PartialEq)]
pub struct YearRow {
    /// Calendar year (`Time` column).
    pub year: i32,
    /// Numeric columns that were non-empty for this row.
    pub values: BTreeMap<String, f64>,
}

/// All rows of one country, ordered by year.
#[derive(Clone, Debug, PartialEq)]
pub struct CountryRows {
    /// ISO 3166-1 alpha-3 code.
    pub iso3: String,
    /// UN location name.
    pub location: String,
    /// Rows ordered by ascending year.
    pub rows: Vec<YearRow>,
}

impl CountryRows {
    /// Returns `column` for `year`, if the row and value exist.
    #[must_use]
    pub fn value(&self, year: i32, column: &str) -> Option<f64> {
        self.rows
            .iter()
            .find(|row| row.year == year)?
            .values
            .get(column)
            .copied()
    }

    /// Returns the newest `(year, value)` of `column` not after `last_year`.
    #[must_use]
    pub fn latest_value(&self, column: &str, last_year: i32) -> Option<(i32, f64)> {
        self.rows
            .iter()
            .rev()
            .filter(|row| row.year <= last_year)
            .find_map(|row| row.values.get(column).map(|value| (row.year, *value)))
    }
}

/// Blocking client that streams the WPP medium-variant CSV.
#[derive(Clone, Debug)]
pub struct WppClient {
    http: HttpClient,
    url: String,
}

impl WppClient {
    /// Creates a client for [`MEDIUM_URL`].
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be initialized.
    pub fn new() -> Result<Self, ClientError> {
        Ok(Self {
            http: HttpClient::builder()
                .user_agent(concat!("statsapi-un-wpp/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(150))
                .build()?,
            url: MEDIUM_URL.to_string(),
        })
    }

    /// Reads the gzip CSV from `url` instead of [`MEDIUM_URL`].
    #[must_use]
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    /// Returns the download URL used for attribution.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Streams the download, keeping only the rows of `iso3`.
    ///
    /// Returns `Ok(None)` when the country has no rows.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid code, HTTP failure, or malformed CSV.
    pub fn fetch_country(&self, iso3: &str) -> Result<Option<CountryRows>, ClientError> {
        validate_country(iso3)?;
        let response = self.http.get(&self.url).send()?.error_for_status()?;
        parse_csv(MultiGzDecoder::new(response), iso3)
    }
}

/// Parses a WPP indicators CSV (plain text) keeping only `iso3` rows.
///
/// # Errors
///
/// Returns an error for an invalid code, malformed CSV or missing columns.
pub fn parse_csv<R: Read>(reader: R, iso3: &str) -> Result<Option<CountryRows>, ClientError> {
    validate_country(iso3)?;
    let mut csv = csv::ReaderBuilder::new().flexible(true).from_reader(reader);
    let headers = csv.headers()?.clone();
    let position = |name: &'static str| {
        headers
            .iter()
            .position(|header| header.trim_start_matches('\u{feff}') == name)
            .ok_or(ClientError::MissingColumn(name))
    };
    let (iso_at, time_at, name_at) = (
        position("ISO3_code")?,
        position("Time")?,
        position("Location")?,
    );
    let numeric: Vec<(usize, &str)> = headers
        .iter()
        .enumerate()
        .filter(|(at, _)| ![iso_at, time_at, name_at].contains(at))
        .map(|(at, header)| (at, header.trim_start_matches('\u{feff}')))
        .collect();
    let mut location = String::new();
    let mut rows = Vec::new();
    let mut record = csv::StringRecord::new();
    while csv.read_record(&mut record)? {
        if record.get(iso_at) != Some(iso3) {
            continue;
        }
        let Some(year) = record
            .get(time_at)
            .and_then(|year| year.parse::<i32>().ok())
        else {
            continue;
        };
        if location.is_empty() {
            location = record.get(name_at).unwrap_or(iso3).to_string();
        }
        let values = numeric
            .iter()
            .filter_map(|(at, header)| {
                let number = record.get(*at)?.parse::<f64>().ok()?;
                Some(((*header).to_string(), number))
            })
            .collect();
        rows.push(YearRow { year, values });
    }
    if rows.is_empty() {
        return Ok(None);
    }
    rows.sort_by_key(|row| row.year);
    Ok(Some(CountryRows {
        iso3: iso3.to_string(),
        location,
        rows,
    }))
}

fn validate_country(country: &str) -> Result<(), ClientError> {
    if country.len() == 3 && country.bytes().all(|byte| byte.is_ascii_uppercase()) {
        Ok(())
    } else {
        Err(ClientError::InvalidCountry(country.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/wpp-medium-sample.csv");

    #[test]
    fn keeps_only_the_requested_country_and_skips_empty_cells() {
        let rows = parse_csv(FIXTURE.as_bytes(), "FRA")
            .expect("parses")
            .expect("France present");
        assert_eq!(rows.location, "France");
        assert_eq!(rows.rows.len(), 4);
        assert_eq!(rows.value(2050, "MedianAgePop"), Some(44.0));
        assert_eq!(rows.value(2051, "MedianAgePop"), None);
    }

    #[test]
    fn latest_value_stops_at_the_last_estimate_year() {
        let rows = parse_csv(FIXTURE.as_bytes(), "FRA").unwrap().unwrap();
        assert_eq!(
            rows.latest_value("TFR", LAST_ESTIMATE_YEAR),
            Some((2023, 1.45))
        );
        assert_eq!(rows.latest_value("TFR", 2021), None);
    }

    #[test]
    fn unknown_country_is_none_and_bad_code_is_rejected() {
        assert!(parse_csv(FIXTURE.as_bytes(), "ZZZ").unwrap().is_none());
        assert!(matches!(
            parse_csv(FIXTURE.as_bytes(), "fr"),
            Err(ClientError::InvalidCountry(_))
        ));
    }

    #[test]
    fn missing_header_column_is_reported() {
        let error = parse_csv("a,b\n1,2\n".as_bytes(), "FRA").unwrap_err();
        assert!(matches!(error, ClientError::MissingColumn("ISO3_code")));
    }

    #[test]
    #[ignore = "live UN WPP CSV stream (about 16 MB)"]
    fn live_stream_reads_usa_estimate_and_projection() {
        let rows = WppClient::new()
            .expect("HTTP client builds")
            .fetch_country("USA")
            .expect("stream")
            .expect("USA rows");
        assert!(rows.value(LAST_ESTIMATE_YEAR, "TPopulation1July").is_some());
        assert!(rows.value(2050, "MedianAgePop").is_some());
    }
}
