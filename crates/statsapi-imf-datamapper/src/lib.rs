//! Blocking client for the IMF `DataMapper` API (`/external/datamapper/api/v1`).
//!
//! The API ignores a country path segment and returns every country for an
//! indicator (about 120 KB), so the client fetches the indicator once and
//! filters locally. Nothing is cached or written to disk.

use reqwest::StatusCode;
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;

const BASE: &str = "https://www.imf.org/external/datamapper/api/v1";
const MAX_ATTEMPTS: u32 = 4;
/// The IMF edge (Akamai) answers 403 to every other `User-Agent`, including
/// an empty one, so the client identifies as curl.
const USER_AGENT: &str = "curl/8.5.0";

/// Errors returned while requesting or decoding `DataMapper` responses.
#[derive(Debug)]
pub enum Error {
    /// The HTTP client could not be built or a request failed.
    Http(reqwest::Error),
    /// The response body was not valid JSON.
    Json(serde_json::Error),
    /// The server answered with a non-retryable (or exhausted) status.
    HttpStatus(StatusCode),
    /// The JSON did not contain the requested indicator.
    MissingIndicator(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(error) => write!(formatter, "imf datamapper request failed: {error}"),
            Self::Json(error) => write!(formatter, "invalid imf datamapper JSON: {error}"),
            Self::HttpStatus(status) => write!(formatter, "imf datamapper returned HTTP {status}"),
            Self::MissingIndicator(code) => {
                write!(formatter, "imf datamapper response has no indicator {code}")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::HttpStatus(_) | Self::MissingIndicator(_) => None,
        }
    }
}

/// Annual values for one country and indicator, keyed by year.
pub type Series = BTreeMap<u16, f64>;

/// A blocking `DataMapper` client.
#[derive(Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
}

impl Client {
    /// Creates a client.
    ///
    /// # Errors
    /// Returns an error if the underlying HTTP client cannot be built.
    pub fn new() -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(Error::Http)?;
        Ok(Self { http })
    }

    /// The endpoint URL for an indicator (all countries).
    #[must_use]
    pub fn indicator_url(indicator: &str) -> String {
        format!("{BASE}/{indicator}")
    }

    /// Fetches an indicator's annual series for one ISO 3166-1 alpha-3
    /// country; `Ok(None)` means the IMF publishes nothing for it.
    ///
    /// # Errors
    /// Returns an error for transport failures, bad statuses or invalid JSON.
    pub fn country_series(&self, indicator: &str, country: &str) -> Result<Option<Series>, Error> {
        let body = self.get(&Self::indicator_url(indicator))?;
        let payload: Value = serde_json::from_slice(&body).map_err(Error::Json)?;
        parse_series(&payload, indicator, country)
    }

    fn get(&self, url: &str) -> Result<Vec<u8>, Error> {
        for attempt in 0..MAX_ATTEMPTS {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    return response
                        .bytes()
                        .map(|bytes| bytes.to_vec())
                        .map_err(Error::Http);
                }
                Ok(response) => {
                    let status = response.status();
                    if !is_retryable(status) || attempt + 1 == MAX_ATTEMPTS {
                        return Err(Error::HttpStatus(status));
                    }
                }
                Err(error) if attempt + 1 == MAX_ATTEMPTS => return Err(Error::Http(error)),
                Err(_) => {}
            }
            std::thread::sleep(Duration::from_millis(200 * (1 << attempt)));
        }
        unreachable!("retry loop returns on its final attempt")
    }
}

fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// Extracts one country's series from a `DataMapper` indicator response.
///
/// # Errors
/// Returns [`Error::MissingIndicator`] when the response lacks the indicator.
pub fn parse_series(
    payload: &Value,
    indicator: &str,
    country: &str,
) -> Result<Option<Series>, Error> {
    let all = payload
        .pointer(&format!("/values/{indicator}"))
        .and_then(Value::as_object)
        .ok_or_else(|| Error::MissingIndicator(indicator.to_string()))?;
    Ok(all.get(country).and_then(Value::as_object).map(|years| {
        years
            .iter()
            .filter_map(|(year, value)| Some((year.parse().ok()?, value.as_f64()?)))
            .collect()
    }))
}

/// A value for one year.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct YearValue {
    /// Calendar year.
    pub year: u16,
    /// Reported or projected value.
    pub value: f64,
}

/// Picks the observation `offset` years after the latest actual.
///
/// The latest actual is the newest year strictly before `current_year`
/// (WEO treats the current year onward as projection). `offset` 0 is that
/// actual; 1 and 2 are the next forecast years.
#[must_use]
pub fn horizon(series: &Series, current_year: u16, offset: u16) -> Option<YearValue> {
    let actual = *series.range(..current_year).next_back()?.0;
    let year = actual.checked_add(offset)?;
    series.get(&year).map(|&value| YearValue { year, value })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/ngdp_rpch.json"))
            .expect("fixture is valid JSON")
    }

    #[test]
    fn filters_one_country_from_the_all_country_payload() {
        let series = parse_series(&fixture(), "NGDP_RPCH", "USA")
            .expect("indicator present")
            .expect("USA present");
        assert_eq!(series.get(&2025), Some(&2.3));
        assert!(parse_series(&fixture(), "NGDP_RPCH", "ZZZ")
            .expect("indicator present")
            .is_none());
    }

    #[test]
    fn unknown_indicator_is_an_error() {
        assert!(matches!(
            parse_series(&fixture(), "NOPE", "USA"),
            Err(Error::MissingIndicator(_))
        ));
    }

    #[test]
    fn horizon_selects_latest_actual_and_next_two_years() {
        let series = parse_series(&fixture(), "NGDP_RPCH", "DEU")
            .expect("indicator present")
            .expect("DEU present");
        assert_eq!(horizon(&series, 2026, 0).map(|v| v.year), Some(2025));
        assert_eq!(horizon(&series, 2026, 1).map(|v| v.year), Some(2026));
        assert_eq!(horizon(&series, 2026, 2).map(|v| v.year), Some(2027));
        assert_eq!(horizon(&series, 2000, 0), None);
    }
}
