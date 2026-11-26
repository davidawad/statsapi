//! Blocking client for Ember's public electricity generation CSV downloads.
//!
//! Ember's keyed API (`api.ember-energy.org`) rejects keyless calls, so this
//! client streams the public release CSVs
//! (`files.ember-energy.org/public-downloads/generation/outputs/`) through a
//! CSV reader and keeps only the requested country's rows. Nothing is written
//! to disk.

use std::fmt;
use std::io::Read;
use std::time::Duration;

use reqwest::blocking::Client as HttpClient;

const BASE: &str = "https://files.ember-energy.org/public-downloads/generation/outputs";

/// Which release file to stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dataset {
    /// `release_generation_yearly_global.csv` (periods are `YYYY`).
    Yearly,
    /// `release_generation_monthly_global.csv` (periods are `YYYY-MM-DD`).
    Monthly,
}

impl Dataset {
    fn file(self) -> &'static str {
        match self {
            Self::Yearly => "release_generation_yearly_global.csv",
            Self::Monthly => "release_generation_monthly_global.csv",
        }
    }
}

/// A numeric column of a [`Row`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Field {
    /// Generation in `TWh`.
    Generation,
    /// Generation change against the same period a year earlier, in percent.
    GenerationYoyPct,
    /// Share of total generation in percent.
    Share,
    /// Change of the share against a year earlier, in percentage points.
    ShareYoyPp,
    /// Emissions intensity in `gCO2e`/kWh.
    Intensity,
}

/// One country, period and electricity-source row.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// Country name.
    pub area: String,
    /// ISO 3166-1 alpha-3 code.
    pub iso3: String,
    /// `YYYY` for yearly rows, `YYYY-MM-DD` (first of month) for monthly rows.
    pub period: String,
    /// Series name such as `Coal`, `Total generation` or `Demand`.
    pub source: String,
    /// Whether Ember marks the series as an aggregate of other series.
    pub aggregated: bool,
    /// Generation in `TWh`.
    pub generation_twh: Option<f64>,
    /// Generation year-over-year change in percent.
    pub generation_yoy_pct: Option<f64>,
    /// Share of generation in percent.
    pub share_pct: Option<f64>,
    /// Share year-over-year change in percentage points.
    pub share_yoy_pp: Option<f64>,
    /// Emissions intensity in `gCO2e`/kWh.
    pub intensity_g_per_kwh: Option<f64>,
}

impl Row {
    /// Reads one numeric column.
    #[must_use]
    pub fn get(&self, field: Field) -> Option<f64> {
        match field {
            Field::Generation => self.generation_twh,
            Field::GenerationYoyPct => self.generation_yoy_pct,
            Field::Share => self.share_pct,
            Field::ShareYoyPp => self.share_yoy_pp,
            Field::Intensity => self.intensity_g_per_kwh,
        }
    }
}

/// Errors from building, fetching or decoding an Ember download.
#[derive(Debug)]
#[non_exhaustive]
pub enum ClientError {
    /// The country code is not an uppercase ISO alpha-3 code.
    InvalidCountry(String),
    /// An HTTP request failed after retries.
    Http(reqwest::Error),
    /// The CSV could not be read.
    Csv(csv::Error),
    /// The CSV header lacks a column this client needs.
    MissingColumn(&'static str),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCountry(code) => write!(f, "invalid ISO alpha-3 country code: {code}"),
            Self::Http(error) => write!(f, "Ember request failed: {error}"),
            Self::Csv(error) => write!(f, "invalid Ember CSV: {error}"),
            Self::MissingColumn(name) => write!(f, "Ember CSV lacks column {name}"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Csv(error) => Some(error),
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

/// Streaming Ember client. It has no cache; every call reads the live file.
#[derive(Clone, Debug)]
pub struct EmberClient {
    http: HttpClient,
    base: String,
    max_retries: usize,
}

impl EmberClient {
    /// Creates a client with a 120 s timeout and two retries.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn new() -> Result<Self, ClientError> {
        Ok(Self {
            http: HttpClient::builder()
                .user_agent(concat!("statsapi-ember/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(120))
                .build()?,
            base: BASE.to_string(),
            max_retries: 2,
        })
    }

    /// Points the client at another host (tests, mirrors).
    #[must_use]
    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base = base.into().trim_end_matches('/').to_string();
        self
    }

    /// The download URL for a dataset, for attribution.
    #[must_use]
    pub fn url(&self, dataset: Dataset) -> String {
        format!("{}/{}", self.base, dataset.file())
    }

    /// Streams a dataset and returns the URL with the country's rows.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid country code, network failure, HTTP
    /// error status, or malformed CSV.
    pub fn fetch_country(
        &self,
        dataset: Dataset,
        iso3: &str,
    ) -> Result<(String, Vec<Row>), ClientError> {
        validate_country(iso3)?;
        let url = self.url(dataset);
        for attempt in 0..=self.max_retries {
            let outcome = self
                .http
                .get(&url)
                .send()
                .and_then(reqwest::blocking::Response::error_for_status);
            match outcome {
                Ok(response) => return Ok((url, parse_country_rows(response, iso3)?)),
                Err(error) if attempt < self.max_retries && retryable(&error) => {
                    std::thread::sleep(Duration::from_millis(500 * (1 << attempt)));
                }
                Err(error) => return Err(ClientError::Http(error)),
            }
        }
        unreachable!("the final attempt returns")
    }
}

fn retryable(error: &reqwest::Error) -> bool {
    error
        .status()
        .is_none_or(|status| status.as_u16() == 429 || status.is_server_error())
}

fn validate_country(iso3: &str) -> Result<(), ClientError> {
    if iso3.len() == 3 && iso3.bytes().all(|byte| byte.is_ascii_uppercase()) {
        Ok(())
    } else {
        Err(ClientError::InvalidCountry(iso3.to_string()))
    }
}

/// Reads a release CSV from any reader, keeping the rows of one country.
///
/// # Errors
///
/// Returns an error when the CSV is malformed or lacks a required column.
pub fn parse_country_rows<R: Read>(reader: R, iso3: &str) -> Result<Vec<Row>, ClientError> {
    let mut csv = csv::Reader::from_reader(reader);
    let headers = csv.headers()?.clone();
    let column = |name: &'static str| {
        headers
            .iter()
            .position(|header| header == name)
            .ok_or(ClientError::MissingColumn(name))
    };
    let area = column("Area")?;
    let code = column("ISO 3 code")?;
    let period = column("Year").or_else(|_| column("Date"))?;
    let source = column("Electricity source")?;
    let aggregated = column("Is aggregated source")?;
    let generation = column("Generation (TWh)")?;
    let generation_yoy = column("Generation YoY change (%)")?;
    let share = column("Share of generation (%)")?;
    let share_yoy = column("Share of generation YoY change (% points)")?;
    let intensity = column("Emissions intensity (gCO2e/kWh)")?;
    let mut rows = Vec::new();
    let mut record = csv::StringRecord::new();
    while csv.read_record(&mut record)? {
        if record.get(code) != Some(iso3) {
            continue;
        }
        let text = |index: usize| record.get(index).unwrap_or_default().to_string();
        let number = |index: usize| record.get(index).and_then(|cell| cell.parse::<f64>().ok());
        rows.push(Row {
            area: text(area),
            iso3: iso3.to_string(),
            period: text(period),
            source: text(source),
            aggregated: record.get(aggregated) == Some("True"),
            generation_twh: number(generation),
            generation_yoy_pct: number(generation_yoy),
            share_pct: number(share),
            share_yoy_pp: number(share_yoy),
            intensity_g_per_kwh: number(intensity),
        });
    }
    Ok(rows)
}

/// The newest row of `series` whose `field` is present, with that value.
#[must_use]
pub fn latest<'a>(rows: &'a [Row], series: &str, field: Field) -> Option<(&'a Row, f64)> {
    rows.iter()
        .filter(|row| row.source == series)
        .filter_map(|row| row.get(field).map(|value| (row, value)))
        .max_by(|left, right| left.0.period.cmp(&right.0.period))
}

/// The row of `series` one year before `period` (yearly datasets only).
#[must_use]
pub fn year_before<'a>(rows: &'a [Row], series: &str, period: &str) -> Option<&'a Row> {
    let year: i32 = period.get(..4)?.parse().ok()?;
    let wanted = format!("{}{}", year - 1, period.get(4..)?);
    rows.iter()
        .find(|row| row.source == series && row.period == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    const YEARLY: &str = include_str!("../tests/fixtures/yearly-usa.csv");
    const MONTHLY: &str = include_str!("../tests/fixtures/monthly-usa.csv");

    #[test]
    fn keeps_only_the_requested_country() {
        let rows = parse_country_rows(YEARLY.as_bytes(), "USA").expect("parse");
        assert_ne!(rows.len(), 0);
        assert!(rows.iter().all(|row| row.iso3 == "USA"));
        let france = parse_country_rows(YEARLY.as_bytes(), "FRA").expect("parse");
        assert!(france.iter().all(|row| row.iso3 == "FRA"));
        assert_eq!(france.len(), 1);
    }

    #[test]
    fn latest_picks_the_newest_year_with_a_value() {
        let rows = parse_country_rows(YEARLY.as_bytes(), "USA").expect("parse");
        let (row, share) = latest(&rows, "Coal", Field::Share).expect("coal share");
        assert_eq!(row.period, "2025");
        assert!((share - 16.309).abs() < 1e-9);
        let (_, intensity) = latest(&rows, "Total generation", Field::Intensity).expect("ci");
        assert!(intensity > 0.0);
        assert!(latest(&rows, "Demand", Field::Intensity).is_none());
    }

    #[test]
    fn year_before_finds_the_prior_period() {
        let rows = parse_country_rows(YEARLY.as_bytes(), "USA").expect("parse");
        assert_eq!(
            year_before(&rows, "Coal", "2025").map(|row| row.period.as_str()),
            Some("2024")
        );
        assert!(year_before(&rows, "Coal", "1990").is_none());
    }

    #[test]
    fn monthly_periods_are_dates() {
        let rows = parse_country_rows(MONTHLY.as_bytes(), "USA").expect("parse");
        let (row, _) = latest(&rows, "Demand", Field::Generation).expect("demand");
        assert_eq!(row.period, "2026-10-01");
    }

    #[test]
    fn rejects_bad_country_and_missing_column() {
        assert!(validate_country("usa").is_err());
        assert!(validate_country("US").is_err());
        let error = parse_country_rows("a,b\n1,2\n".as_bytes(), "USA").expect_err("no columns");
        assert!(matches!(error, ClientError::MissingColumn(_)));
    }

    #[test]
    #[ignore = "live Ember CSV download"]
    fn live_streams_yearly_and_monthly_rows() {
        let client = EmberClient::new().expect("client");
        for dataset in [Dataset::Yearly, Dataset::Monthly] {
            let (url, rows) = client.fetch_country(dataset, "USA").expect("fetch");
            assert!(url.starts_with("https://files.ember-energy.org/"));
            assert!(latest(&rows, "Total generation", Field::Intensity).is_some());
        }
    }
}
