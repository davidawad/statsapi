//! Blocking client for the keyless NASA POWER daily point API.
//!
//! The client queries the live API and keeps nothing on disk.

use reqwest::StatusCode;
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_RETRIES: u32 = 3;
const DEFAULT_BASE: &str = "https://power.larc.nasa.gov/api/temporal/daily/point";

/// Errors returned while querying NASA POWER.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The HTTP request failed.
    Http(reqwest::Error),
    /// POWER returned a non-success status.
    Status { url: String, status: u16 },
    /// The response was not the documented daily point JSON.
    Decode(String),
    /// A date was not `YYYYMMDD`.
    Date(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(error) => write!(f, "NASA POWER request failed: {error}"),
            Self::Status { url, status } => {
                write!(f, "NASA POWER returned HTTP {status} for {url}")
            }
            Self::Decode(message) => write!(f, "NASA POWER response is invalid: {message}"),
            Self::Date(text) => write!(f, "invalid date {text:?}, want YYYYMMDD or YYYY-MM-DD"),
        }
    }
}
impl std::error::Error for Error {}

/// A proleptic Gregorian calendar date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    /// Parses `YYYYMMDD` or `YYYY-MM-DD`.
    ///
    /// # Errors
    /// Returns [`Error::Date`] when the text is not a real calendar date.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let digits: String = text.chars().filter(|c| *c != '-').collect();
        let bad = || Error::Date(text.to_owned());
        if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        let year = digits[..4].parse().map_err(|_| bad())?;
        let month: u32 = digits[4..6].parse().map_err(|_| bad())?;
        let day: u32 = digits[6..].parse().map_err(|_| bad())?;
        let date = Self { year, month, day };
        ((1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month))
            .then_some(date)
            .ok_or_else(bad)
    }

    /// Days since 1970-01-01.
    #[must_use]
    pub fn to_days(self) -> i64 {
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y.rem_euclid(400);
        let m = i64::from(self.month);
        let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// The date `days` after 1970-01-01.
    #[must_use]
    pub fn from_days(days: i64) -> Self {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        Self {
            year: i32::try_from(year).unwrap_or(i32::MAX),
            month: u32::try_from(month).unwrap_or(1),
            day: u32::try_from(day).unwrap_or(1),
        }
    }

    /// Adds a signed number of days.
    #[must_use]
    pub fn add_days(self, days: i64) -> Self {
        Self::from_days(self.to_days() + days)
    }

    /// Formats as `YYYYMMDD`, the form the POWER API uses.
    #[must_use]
    pub fn compact(self) -> String {
        format!("{:04}{:02}{:02}", self.year, self.month, self.day)
    }

    /// Formats as `YYYY-MM-DD`.
    #[must_use]
    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// One daily point request.
#[derive(Debug, Clone)]
pub struct DailyQuery<'a> {
    pub latitude: f64,
    pub longitude: f64,
    pub start: Date,
    pub end: Date,
    /// POWER parameter names such as `T2M`, `T2M_MAX`, `PRECTOTCORR`.
    pub parameters: &'a [&'a str],
}

/// Decoded daily point response.
#[derive(Debug, Clone)]
pub struct Daily {
    /// The URL that produced this response (no secrets: the API is keyless).
    pub url: String,
    /// Grid latitude/longitude/elevation POWER served.
    pub coordinates: Option<(f64, f64, f64)>,
    /// POWER's missing-value marker (normally -999).
    pub fill_value: f64,
    /// Units per parameter, from the response header.
    pub units: BTreeMap<String, String>,
    series: BTreeMap<String, BTreeMap<Date, f64>>,
}

impl Daily {
    /// Decodes the JSON body of a daily point response.
    ///
    /// # Errors
    /// Returns [`Error::Decode`] when the documented layout is absent.
    pub fn from_json(url: &str, body: &serde_json::Value) -> Result<Self, Error> {
        let bad = |what: &str| Error::Decode(what.to_owned());
        let parameter = body["properties"]["parameter"]
            .as_object()
            .ok_or_else(|| bad("properties.parameter missing"))?;
        let fill_value = body["header"]["fill_value"].as_f64().unwrap_or(-999.0);
        let mut series = BTreeMap::new();
        for (name, days) in parameter {
            let days = days
                .as_object()
                .ok_or_else(|| bad("parameter is not an object"))?;
            let mut map = BTreeMap::new();
            for (day, value) in days {
                let Some(number) = value.as_f64() else {
                    continue;
                };
                if (number - fill_value).abs() < f64::EPSILON {
                    continue;
                }
                map.insert(Date::parse(day)?, number);
            }
            series.insert(name.clone(), map);
        }
        let units = body["parameters"]
            .as_object()
            .map_or_else(BTreeMap::new, |parameters| {
                parameters
                    .iter()
                    .filter_map(|(name, meta)| {
                        meta["units"]
                            .as_str()
                            .map(|unit| (name.clone(), unit.to_owned()))
                    })
                    .collect()
            });
        let coordinates = body["geometry"]["coordinates"].as_array().and_then(|c| {
            Some((
                c.first()?.as_f64()?,
                c.get(1)?.as_f64()?,
                c.get(2)?.as_f64()?,
            ))
        });
        Ok(Self {
            url: url.to_owned(),
            coordinates,
            fill_value,
            units,
            series,
        })
    }

    /// Valid (non-fill) daily values of one parameter, ordered by date.
    #[must_use]
    pub fn values(&self, parameter: &str) -> Vec<(Date, f64)> {
        self.series
            .get(parameter)
            .map_or_else(Vec::new, |map| map.iter().map(|(d, v)| (*d, *v)).collect())
    }
}

/// Blocking NASA POWER client.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    base: String,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Creates a client for the public POWER service.
    #[must_use]
    pub fn new() -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(90))
            .build()
            .unwrap_or_default();
        Self {
            http,
            base: DEFAULT_BASE.to_owned(),
        }
    }

    /// Points the client at another base URL (tests, mirrors).
    #[must_use]
    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base = base.into();
        self
    }

    /// Builds the request URL for a query.
    #[must_use]
    pub fn url(&self, query: &DailyQuery<'_>) -> String {
        format!(
            "{}?parameters={}&community=AG&longitude={:.4}&latitude={:.4}&start={}&end={}&format=JSON",
            self.base,
            query.parameters.join(","),
            query.longitude,
            query.latitude,
            query.start.compact(),
            query.end.compact()
        )
    }

    /// Fetches daily values for one point, retrying 429, 5xx and transport errors.
    ///
    /// # Errors
    /// Returns an error for HTTP failures or an undecodable body.
    pub fn daily_point(&self, query: &DailyQuery<'_>) -> Result<Daily, Error> {
        let url = self.url(query);
        for attempt in 0..=MAX_RETRIES {
            match self.http.get(&url).send() {
                Ok(response) if response.status().is_success() => {
                    let bytes = response.bytes().map_err(Error::Http)?;
                    let body: serde_json::Value = serde_json::from_slice(&bytes)
                        .map_err(|error| Error::Decode(error.to_string()))?;
                    return Daily::from_json(&url, &body);
                }
                Ok(response) => {
                    let status = response.status();
                    if !(status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                        || attempt == MAX_RETRIES
                    {
                        return Err(Error::Status {
                            url,
                            status: status.as_u16(),
                        });
                    }
                }
                Err(error) if attempt == MAX_RETRIES => return Err(Error::Http(error)),
                Err(_) => {}
            }
            let jitter = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |now| u64::from(now.subsec_millis()) % 250);
            std::thread::sleep(Duration::from_millis(
                500_u64.saturating_mul(1 << attempt) + jitter,
            ));
        }
        Err(Error::Decode("retry loop exhausted".to_owned()))
    }
}
