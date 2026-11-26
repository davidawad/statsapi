//! Blocking client for the keyless BIS Data Portal SDMX API (`stats.bis.org/api/v2`).
//!
//! The client reads four BIS statistical flows by two-letter country code and
//! returns the newest observation (or a short history) with the exact request
//! URL, so callers can cite it. Nothing is cached or written to disk.

use std::time::Duration;
use thiserror::Error;

/// Default API root of the BIS Data Portal.
pub const DEFAULT_BASE: &str = "https://stats.bis.org/api/v2";

/// Media type that makes the portal answer with SDMX-CSV.
const SDMX_CSV: &str = "application/vnd.sdmx.data+csv;version=1.0.0";

/// Retries after the first attempt for 429, 5xx and transport failures.
const DEFAULT_RETRIES: u32 = 3;

/// Base back-off between retries; attempt `n` waits up to `BACKOFF_MS << n`.
const BACKOFF_MS: u64 = 400;

/// The BIS statistical series this client reads.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Series {
    /// Credit-to-GDP gap in percentage points (`WS_CREDIT_GAP`, private non-financial sector).
    CreditGap,
    /// Total credit to the private non-financial sector, % of GDP (`WS_TC`, market value).
    CreditToGdp,
    /// Residential property prices, real, year-on-year change in per cent (`WS_SPP`).
    RealPropertyPriceYoy,
    /// Debt service ratio of the private non-financial sector, per cent of income (`WS_DSR`).
    DebtServiceRatio,
}

impl Series {
    /// Every series the client knows, in a stable order.
    pub const ALL: [Self; 4] = [
        Self::CreditGap,
        Self::CreditToGdp,
        Self::RealPropertyPriceYoy,
        Self::DebtServiceRatio,
    ];

    /// The portal dataflow path segment, `<agency>/<id>/<version>`.
    #[must_use]
    pub const fn dataflow(self) -> &'static str {
        match self {
            Self::CreditGap => "BIS/WS_CREDIT_GAP/1.0",
            Self::CreditToGdp => "BIS/WS_TC/2.0",
            Self::RealPropertyPriceYoy => "BIS/WS_SPP/1.0",
            Self::DebtServiceRatio => "BIS/WS_DSR/1.0",
        }
    }

    /// The SDMX series key for a two-letter BIS country code (`Q.<country>...`).
    #[must_use]
    pub fn key(self, country: &str) -> String {
        match self {
            Self::CreditGap => format!("Q.{country}.P.A.C"),
            Self::CreditToGdp => format!("Q.{country}.P.A.M.770.A"),
            Self::RealPropertyPriceYoy => format!("Q.{country}.R.771"),
            Self::DebtServiceRatio => format!("Q.{country}.P"),
        }
    }

    /// Unit of the reported value.
    #[must_use]
    pub const fn unit(self) -> &'static str {
        match self {
            Self::CreditGap => "percentage_points_of_gdp",
            Self::CreditToGdp => "percent_of_gdp",
            Self::RealPropertyPriceYoy => "percent_yoy",
            Self::DebtServiceRatio => "percent_of_income",
        }
    }
}

/// One BIS observation.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    /// Period label such as `2026-Q1`.
    pub period: String,
    /// Observed value in the unit of the [`Series`].
    pub value: f64,
}

/// Newest observations of one series with the request URL that produced them.
#[derive(Clone, Debug, PartialEq)]
pub struct SeriesData {
    /// Observations, oldest first.
    pub observations: Vec<Observation>,
    /// The data request URL, for citation.
    pub url: String,
}

impl SeriesData {
    /// The newest observation.
    #[must_use]
    pub fn latest(&self) -> Option<&Observation> {
        self.observations.last()
    }
}

/// Errors from requesting or decoding a BIS response.
#[derive(Debug, Error)]
pub enum Error {
    /// The HTTP transport failed.
    #[error("BIS request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// The portal answered with an unsuccessful status.
    #[error("BIS returned HTTP {status} for {url}")]
    Status {
        /// HTTP status code.
        status: u16,
        /// Request URL.
        url: String,
    },
    /// The portal knows no series for the key (SDMX error 100, HTTP 404).
    #[error("BIS has no series {key} in {dataflow}")]
    NoSeries {
        /// Dataflow path.
        dataflow: &'static str,
        /// Series key.
        key: String,
    },
    /// The response body was not usable SDMX-CSV.
    #[error("BIS response could not be decoded: {0}")]
    Decode(String),
}

/// Blocking BIS Data Portal client.
#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
    base: String,
    retries: u32,
}

impl Client {
    /// Creates a client for the public portal with a 45-second timeout.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] if the HTTP transport cannot be initialized.
    pub fn new() -> Result<Self, Error> {
        Self::with_base(DEFAULT_BASE)
    }

    /// Creates a client for another API root, for tests against a local server.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] if the HTTP transport cannot be initialized.
    pub fn with_base(base: &str) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!("statsapi-bis/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(45))
            .build()?;
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
            retries: DEFAULT_RETRIES,
        })
    }

    /// The data request URL for the newest `last_n` observations of a country.
    #[must_use]
    pub fn data_url(&self, series: Series, country: &str, last_n: u32) -> String {
        format!(
            "{}/data/dataflow/{}/{}?lastNObservations={last_n}&detail=dataonly",
            self.base,
            series.dataflow(),
            series.key(country)
        )
    }

    /// Fetches the newest `last_n` observations of `series` for a two-letter BIS country code.
    ///
    /// Retries 429, 5xx and transport failures with jittered back-off.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NoSeries`] when BIS publishes nothing for the country,
    /// and HTTP, status or decode errors otherwise.
    pub fn series(&self, series: Series, country: &str, last_n: u32) -> Result<SeriesData, Error> {
        let url = self.data_url(series, country, last_n);
        let body = self.get(&url)?;
        let observations = parse_observations(&body)?;
        if observations.is_empty() {
            return Err(Error::NoSeries {
                dataflow: series.dataflow(),
                key: series.key(country),
            });
        }
        Ok(SeriesData { observations, url })
    }

    fn get(&self, url: &str) -> Result<String, Error> {
        let mut attempt = 0;
        loop {
            let outcome = self
                .http
                .get(url)
                .header(reqwest::header::ACCEPT, SDMX_CSV)
                .send();
            match outcome {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return Ok(response.text()?);
                    }
                    if status.as_u16() == 404 {
                        return Ok(String::new());
                    }
                    let transient = status.as_u16() == 429 || status.is_server_error();
                    if !transient || attempt >= self.retries {
                        return Err(Error::Status {
                            status: status.as_u16(),
                            url: url.to_owned(),
                        });
                    }
                }
                Err(error) => {
                    let transient = error.is_timeout() || error.is_connect();
                    if !transient || attempt >= self.retries {
                        return Err(error.into());
                    }
                }
            }
            pause(attempt);
            attempt += 1;
        }
    }
}

/// Sleeps a jittered, exponentially growing interval.
fn pause(attempt: u32) {
    let ceiling = BACKOFF_MS << attempt.min(6);
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| u64::from(elapsed.subsec_nanos()));
    std::thread::sleep(Duration::from_millis(jitter % ceiling));
}

/// Decodes SDMX-CSV into observations, oldest first.
///
/// A body without the `TIME_PERIOD` and `OBS_VALUE` columns (the portal's
/// XML "No structures match" error, or an empty 404 body) decodes to no
/// observations; rows with an empty or non-numeric value are skipped.
///
/// # Errors
///
/// Returns [`Error::Decode`] when the CSV is malformed.
pub fn parse_observations(body: &str) -> Result<Vec<Observation>, Error> {
    if !body.starts_with("DATAFLOW,") {
        return Ok(Vec::new());
    }
    let mut reader = csv::Reader::from_reader(body.as_bytes());
    let headers = reader
        .headers()
        .map_err(|error| Error::Decode(error.to_string()))?
        .clone();
    let column = |name: &str| headers.iter().position(|header| header == name);
    let (Some(period), Some(value)) = (column("TIME_PERIOD"), column("OBS_VALUE")) else {
        return Ok(Vec::new());
    };
    let mut observations = Vec::new();
    for row in reader.records() {
        let row = row.map_err(|error| Error::Decode(error.to_string()))?;
        let parsed = row.get(value).and_then(|text| text.parse::<f64>().ok());
        if let (Some(period), Some(value)) = (row.get(period), parsed) {
            observations.push(Observation {
                period: period.to_owned(),
                value,
            });
        }
    }
    observations.sort_by(|left, right| left.period.cmp(&right.period));
    Ok(observations)
}

/// BIS two-letter code for an ISO 3166-1 alpha-3 country, for the economies BIS publishes.
///
/// Returns `None` for countries outside the BIS credit and property price coverage.
#[must_use]
pub fn bis_country(iso3: &str) -> Option<&'static str> {
    COUNTRIES
        .iter()
        .find_map(|(code3, code2)| (*code3 == iso3).then_some(*code2))
}

/// ISO alpha-3 to BIS (ISO alpha-2) codes for economies in the BIS credit and property flows.
const COUNTRIES: [(&str, &str); 62] = [
    ("ARE", "AE"),
    ("ARG", "AR"),
    ("AUS", "AU"),
    ("AUT", "AT"),
    ("BEL", "BE"),
    ("BGR", "BG"),
    ("BRA", "BR"),
    ("CAN", "CA"),
    ("CHE", "CH"),
    ("CHL", "CL"),
    ("CHN", "CN"),
    ("COL", "CO"),
    ("CYP", "CY"),
    ("CZE", "CZ"),
    ("DEU", "DE"),
    ("DNK", "DK"),
    ("ESP", "ES"),
    ("EST", "EE"),
    ("FIN", "FI"),
    ("FRA", "FR"),
    ("GBR", "GB"),
    ("GRC", "GR"),
    ("HKG", "HK"),
    ("HRV", "HR"),
    ("HUN", "HU"),
    ("IDN", "ID"),
    ("IND", "IN"),
    ("IRL", "IE"),
    ("ISL", "IS"),
    ("ISR", "IL"),
    ("ITA", "IT"),
    ("JPN", "JP"),
    ("KOR", "KR"),
    ("LTU", "LT"),
    ("LUX", "LU"),
    ("LVA", "LV"),
    ("MEX", "MX"),
    ("MKD", "MK"),
    ("MLT", "MT"),
    ("MYS", "MY"),
    ("NLD", "NL"),
    ("NOR", "NO"),
    ("NZL", "NZ"),
    ("PER", "PE"),
    ("PHL", "PH"),
    ("POL", "PL"),
    ("PRT", "PT"),
    ("ROU", "RO"),
    ("RUS", "RU"),
    ("SAU", "SA"),
    ("SGP", "SG"),
    ("SVK", "SK"),
    ("SVN", "SI"),
    ("SWE", "SE"),
    ("THA", "TH"),
    ("TUR", "TR"),
    ("TWN", "TW"),
    ("UKR", "UA"),
    ("USA", "US"),
    ("VNM", "VN"),
    ("ZAF", "ZA"),
    ("CRI", "CR"),
];

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/credit_gap.csv");

    #[test]
    fn parses_sdmx_csv_oldest_first() {
        let rows = parse_observations(FIXTURE).expect("fixture decodes");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].period, "2031-Q1");
        assert_eq!(rows[1].period, "2031-Q2");
        assert!((rows[1].value - (-3.25)).abs() < 1e-9);
    }

    #[test]
    fn xml_error_body_is_no_observations() {
        let body = "<?xml version=\"1.0\" ?>\n<message:Error/>";
        assert_eq!(parse_observations(body).expect("decodes"), Vec::new());
    }

    #[test]
    fn builds_keys_and_urls() {
        let client = Client::new().expect("client builds");
        assert_eq!(Series::CreditToGdp.key("US"), "Q.US.P.A.M.770.A");
        assert_eq!(
            client.data_url(Series::RealPropertyPriceYoy, "DE", 1),
            "https://stats.bis.org/api/v2/data/dataflow/BIS/WS_SPP/1.0/Q.DE.R.771?lastNObservations=1&detail=dataonly"
        );
    }

    #[test]
    fn maps_iso3_to_bis_codes() {
        assert_eq!(bis_country("USA"), Some("US"));
        assert_eq!(bis_country("GBR"), Some("GB"));
        assert_eq!(bis_country("ATA"), None);
    }
}
