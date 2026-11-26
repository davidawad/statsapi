//! Blocking client for the `NOAA` `National Weather Service` `api.weather.gov` service.

use reqwest::StatusCode;
use serde_json::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BASE: &str = "https://api.weather.gov";
const MAX_RETRIES: u32 = 3;

/// Errors returned while querying the `NWS` API.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The HTTP request failed.
    Http(reqwest::Error),
    /// `NWS` returned a non-success status.
    Status { url: String, status: u16 },
    /// The response was not valid JSON.
    Decode(serde_json::Error),
    /// The response lacked an expected member.
    Shape(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(error) => write!(f, "NWS request failed: {error}"),
            Self::Status { url, status } => write!(f, "NWS returned HTTP {status} for {url}"),
            Self::Decode(error) => write!(f, "NWS response JSON is invalid: {error}"),
            Self::Shape(what) => write!(f, "NWS response omitted {what}"),
        }
    }
}
impl std::error::Error for Error {}

/// `CAP` severity of an alert, ordered from least to most severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Severity {
    Unknown,
    Minor,
    Moderate,
    Severe,
    Extreme,
}

impl Severity {
    /// Parses the `CAP` severity word; anything unrecognized is `Unknown`.
    #[must_use]
    pub fn parse(word: &str) -> Self {
        match word {
            "Minor" => Self::Minor,
            "Moderate" => Self::Moderate,
            "Severe" => Self::Severe,
            "Extreme" => Self::Extreme,
            _ => Self::Unknown,
        }
    }
    /// Ordinal code, `Unknown` = 0 through `Extreme` = 4.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Minor => 1,
            Self::Moderate => 2,
            Self::Severe => 3,
            Self::Extreme => 4,
        }
    }
}

/// One active alert covering a point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub id: String,
    pub event: String,
    pub severity: Severity,
    pub headline: Option<String>,
    pub expires: Option<String>,
}

/// One named forecast period (for example "Monday" or "Monday Night").
#[derive(Debug, Clone, PartialEq)]
pub struct Period {
    pub name: String,
    /// Local start time as returned by `NWS` (`2026-11-23T06:00:00-05:00`).
    pub start_time: String,
    pub is_daytime: bool,
    /// Temperature converted to degrees Fahrenheit.
    pub temperature_f: f64,
}

/// A gridpoint forecast.
#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    pub updated: String,
    pub periods: Vec<Period>,
}

impl Forecast {
    /// Calendar date (`YYYY-MM-DD`, local to the grid) of a period.
    fn date_of(period: &Period) -> &str {
        period.start_time.get(..10).unwrap_or("")
    }
    /// The daytime high for a local calendar date.
    #[must_use]
    pub fn high_on(&self, date: &str) -> Option<f64> {
        self.periods
            .iter()
            .find(|p| p.is_daytime && Self::date_of(p) == date)
            .map(|p| p.temperature_f)
    }
    /// The nighttime low of the period that starts on a local calendar date.
    #[must_use]
    pub fn low_on(&self, date: &str) -> Option<f64> {
        self.periods
            .iter()
            .find(|p| !p.is_daytime && Self::date_of(p) == date)
            .map(|p| p.temperature_f)
    }
}

/// Decodes an `alerts/active` `GeoJSON` collection.
///
/// # Errors
/// Returns [`Error::Shape`] when the `features` array is absent.
pub fn parse_alerts(value: &Value) -> Result<Vec<Alert>, Error> {
    let features = value
        .get("features")
        .and_then(Value::as_array)
        .ok_or(Error::Shape("features"))?;
    Ok(features
        .iter()
        .filter_map(|feature| {
            let props = feature.get("properties")?;
            let text = |key: &str| props.get(key).and_then(Value::as_str).map(str::to_owned);
            Some(Alert {
                id: text("id").unwrap_or_default(),
                event: text("event")?,
                severity: Severity::parse(props.get("severity").and_then(Value::as_str)?),
                headline: text("headline"),
                expires: text("expires"),
            })
        })
        .collect())
}

/// Decodes a gridpoint `forecast` document.
///
/// # Errors
/// Returns [`Error::Shape`] when the `periods` array is absent.
pub fn parse_forecast(value: &Value) -> Result<Forecast, Error> {
    let props = value.get("properties").ok_or(Error::Shape("properties"))?;
    let periods = props
        .get("periods")
        .and_then(Value::as_array)
        .ok_or(Error::Shape("periods"))?;
    Ok(Forecast {
        updated: props
            .get("updateTime")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        periods: periods
            .iter()
            .filter_map(|p| {
                let raw = p.get("temperature")?.as_f64()?;
                let celsius = p.get("temperatureUnit").and_then(Value::as_str) == Some("C");
                Some(Period {
                    name: p.get("name")?.as_str()?.to_owned(),
                    start_time: p.get("startTime")?.as_str()?.to_owned(),
                    is_daytime: p.get("isDaytime")?.as_bool()?,
                    temperature_f: if celsius { raw * 9.0 / 5.0 + 32.0 } else { raw },
                })
            })
            .collect(),
    })
}

/// Live `NWS` client. `NWS` requires a descriptive `User-Agent`.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    base: String,
}

impl Client {
    /// Creates a client that identifies itself with `user_agent`.
    #[must_use]
    pub fn new(user_agent: &str) -> Self {
        Self::with_base(user_agent, BASE)
    }
    /// Creates a client against another base URL (tests, mirrors).
    #[must_use]
    pub fn with_base(user_agent: &str, base: &str) -> Self {
        let http = reqwest::blocking::Client::builder()
            .user_agent(user_agent)
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
        }
    }
    /// URL of the active-alert query for a point.
    #[must_use]
    pub fn alerts_url(&self, lat: f64, lon: f64) -> String {
        format!("{}/alerts/active?point={lat:.4},{lon:.4}", self.base)
    }
    /// URL of the point metadata query that locates the forecast grid.
    #[must_use]
    pub fn point_url(&self, lat: f64, lon: f64) -> String {
        format!("{}/points/{lat:.4},{lon:.4}", self.base)
    }
    /// Fetches the alerts active at a point.
    ///
    /// # Errors
    /// Returns an error for HTTP, decoding or unexpected-shape failures.
    pub fn alerts_active(&self, lat: f64, lon: f64) -> Result<Vec<Alert>, Error> {
        parse_alerts(&self.get_json(&self.alerts_url(lat, lon))?)
    }
    /// Fetches the gridpoint forecast covering a point, returning it with its URL.
    ///
    /// # Errors
    /// Returns an error for HTTP, decoding or unexpected-shape failures. A point
    /// outside `NWS` coverage is [`Error::Status`] with status 404.
    pub fn forecast(&self, lat: f64, lon: f64) -> Result<(Forecast, String), Error> {
        let point = self.get_json(&self.point_url(lat, lon))?;
        let url = point
            .pointer("/properties/forecast")
            .and_then(Value::as_str)
            .ok_or(Error::Shape("properties.forecast"))?
            .to_owned();
        let forecast = parse_forecast(&self.get_json(&url)?)?;
        Ok((forecast, url))
    }
    /// Fetches a JSON document with jittered retry on 429, 5xx and transport errors.
    ///
    /// # Errors
    /// Returns an error for HTTP or decoding failures.
    pub fn get_json(&self, url: &str) -> Result<Value, Error> {
        for attempt in 0..=MAX_RETRIES {
            let retry = match self
                .http
                .get(url)
                .header("Accept", "application/geo+json")
                .send()
            {
                Ok(response) if response.status().is_success() => {
                    let bytes = response.bytes().map_err(Error::Http)?;
                    return serde_json::from_slice(&bytes).map_err(Error::Decode);
                }
                Ok(response) => {
                    let status = response.status();
                    if !(status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                        || attempt == MAX_RETRIES
                    {
                        return Err(Error::Status {
                            url: url.to_owned(),
                            status: status.as_u16(),
                        });
                    }
                    true
                }
                Err(error) if attempt == MAX_RETRIES => return Err(Error::Http(error)),
                Err(_) => true,
            };
            if retry {
                let jitter = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |now| u64::from(now.subsec_millis()) % 250);
                std::thread::sleep(Duration::from_millis(
                    250_u64.saturating_mul(1 << attempt) + jitter,
                ));
            }
        }
        Err(Error::Shape("response"))
    }
}
