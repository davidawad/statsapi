//! Blocking client for the US Drought Monitor (USDM) data services.
//!
//! The service is keyless and queried live; nothing is cached or written to disk.
//! Statistics are requested in the cumulative "traditional" format, so `d0` is the
//! percent of area in D0-D4, `d1` the percent in D1-D4, and so on.

use std::time::Duration;

const DEFAULT_BASE_URL: &str = "https://usdmdataservices.unl.edu/api";
const MAX_ATTEMPTS: u32 = 3;
const WEEK_DAYS: i64 = 7;

/// Errors returned by the client.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The HTTP request failed.
    Http(reqwest::Error),
    /// The service returned a non-success status.
    Status { url: String, status: u16 },
    /// The response was not the expected JSON.
    Decode(String),
    /// The service returned no weeks for the area and window.
    NoData { area: String },
    /// A date string was not `YYYY-MM-DD`.
    BadDate(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(error) => write!(f, "USDM request failed: {error}"),
            Self::Status { url, status } => write!(f, "USDM returned HTTP {status} for {url}"),
            Self::Decode(message) => write!(f, "USDM response is invalid: {message}"),
            Self::NoData { area } => write!(f, "USDM has no data for {area}"),
            Self::BadDate(text) => write!(f, "date {text:?} is not YYYY-MM-DD"),
        }
    }
}
impl std::error::Error for Error {}

/// A calendar date, stored as days since 1970-01-01.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date(i64);

impl Date {
    /// Parses `YYYY-MM-DD`, ignoring any trailing time component.
    ///
    /// # Errors
    /// Returns [`Error::BadDate`] when the text is not a valid date.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let bad = || Error::BadDate(text.to_owned());
        let day_part = text.get(..10).ok_or_else(bad)?;
        let mut parts = day_part.split('-');
        let (Some(y), Some(m), Some(d), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(bad());
        };
        let year: i64 = y.parse().map_err(|_| bad())?;
        let month: i64 = m.parse().map_err(|_| bad())?;
        let day: i64 = d.parse().map_err(|_| bad())?;
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return Err(bad());
        }
        Ok(Self::from_civil(year, month, day))
    }
    /// Today's date in UTC.
    #[must_use]
    pub fn today() -> Self {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        Self(i64::try_from(seconds / 86_400).unwrap_or(0))
    }
    /// Returns this date shifted by `days` (negative goes back).
    #[must_use]
    pub fn plus_days(self, days: i64) -> Self {
        Self(self.0 + days)
    }
    /// Whole days from `other` to `self`.
    #[must_use]
    pub fn days_since(self, other: Self) -> i64 {
        self.0 - other.0
    }
    fn from_civil(year: i64, month: i64, day: i64) -> Self {
        let y = if month <= 2 { year - 1 } else { year };
        let era = y.div_euclid(400);
        let yoe = y.rem_euclid(400);
        let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        Self(era * 146_097 + doe - 719_468)
    }
    fn civil(self) -> (i64, i64, i64) {
        let z = self.0 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        (year, month, day)
    }
}
impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (year, month, day) = self.civil();
        write!(f, "{year:04}-{month:02}-{day:02}")
    }
}

/// Geography the statistics are aggregated over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    /// A county, identified by its 5-digit FIPS code.
    County,
    /// A state, identified by its 2-digit FIPS code.
    State,
}
impl Area {
    fn path(self) -> &'static str {
        match self {
            Self::County => "CountyStatistics",
            Self::State => "StateStatistics",
        }
    }
}

/// One weekly USDM map: cumulative percent of area at or above each category.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Week {
    /// The Tuesday the map is valid for.
    pub map_date: Date,
    /// Percent of area in D0-D4.
    pub d0: f64,
    /// Percent of area in D1-D4.
    pub d1: f64,
    /// Percent of area in D2-D4.
    pub d2: f64,
    /// Percent of area in D3-D4.
    pub d3: f64,
    /// Percent of area in D4.
    pub d4: f64,
}

/// Latest week plus the comparisons the indicator library serves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summary {
    /// The most recent week.
    pub latest: Week,
    /// Latest D2-D4 percent minus the value 4 weeks earlier, when that week exists.
    pub d2_change_4w: Option<f64>,
    /// Latest D2-D4 percent minus the value 52 weeks earlier, when that week exists.
    pub d2_change_1y: Option<f64>,
    /// Weeks with any area in D2-D4 among the 52 weeks ending at the latest week.
    pub weeks_d2_plus_1y: u32,
}

/// Parses a USDM statistics response into weeks, newest first.
///
/// # Errors
/// Returns [`Error::Decode`] for malformed JSON or rows missing a date or category.
pub fn parse_weeks(body: &str) -> Result<Vec<Week>, Error> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|error| Error::Decode(error.to_string()))?;
    let rows = value
        .as_array()
        .ok_or_else(|| Error::Decode("top level is not an array".into()))?;
    let mut weeks = rows
        .iter()
        .map(|row| {
            let number = |key: &str| {
                row.get(key)
                    .and_then(serde_json::Value::as_f64)
                    .ok_or_else(|| Error::Decode(format!("row lacks numeric {key}")))
            };
            let date = row
                .get("mapDate")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| Error::Decode("row lacks mapDate".into()))?;
            Ok(Week {
                map_date: Date::parse(date)?,
                d0: number("d0")?,
                d1: number("d1")?,
                d2: number("d2")?,
                d3: number("d3")?,
                d4: number("d4")?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    weeks.sort_by_key(|week| std::cmp::Reverse(week.map_date));
    Ok(weeks)
}

/// Computes the latest-week summary from weeks (any order); `None` when empty.
#[must_use]
pub fn summarize(weeks: &[Week]) -> Option<Summary> {
    let latest = *weeks.iter().max_by_key(|week| week.map_date)?;
    let at = |days: i64| {
        let target = latest.map_date.plus_days(-days);
        weeks.iter().find(|week| week.map_date == target)
    };
    let window_start = latest.map_date.plus_days(-52 * WEEK_DAYS);
    let weeks_d2_plus_1y = weeks
        .iter()
        .filter(|week| week.map_date > window_start && week.map_date <= latest.map_date)
        .filter(|week| week.d2 > 0.0)
        .count();
    Some(Summary {
        latest,
        d2_change_4w: at(4 * WEEK_DAYS).map(|past| latest.d2 - past.d2),
        d2_change_1y: at(52 * WEEK_DAYS).map(|past| latest.d2 - past.d2),
        weeks_d2_plus_1y: u32::try_from(weeks_d2_plus_1y).unwrap_or(u32::MAX),
    })
}

/// Blocking USDM client. Every call is a live request; nothing is persisted.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    base_url: String,
}
impl Client {
    /// Creates a client for the public USDM service.
    ///
    /// # Errors
    /// Returns an error when the HTTP client cannot be built.
    pub fn new() -> Result<Self, Error> {
        Self::with_base_url(DEFAULT_BASE_URL)
    }
    /// Creates a client for a service at `base_url` (used by tests).
    ///
    /// # Errors
    /// Returns an error when the HTTP client cannot be built.
    pub fn with_base_url(base_url: &str) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(Error::Http)?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
        })
    }
    /// URL of the weekly percent-area query for `aoi` between two dates.
    #[must_use]
    pub fn url(&self, area: Area, aoi: &str, start: Date, end: Date) -> String {
        let us = |date: Date| {
            let (year, month, day) = date.civil();
            format!("{month}/{day}/{year}")
        };
        format!(
            "{}/{}/GetDroughtSeverityStatisticsByAreaPercent?aoi={aoi}&startdate={}&enddate={}&statisticsType=1",
            self.base_url,
            area.path(),
            us(start),
            us(end)
        )
    }
    /// Fetches weekly cumulative percent-area rows for `aoi`, newest first.
    ///
    /// # Errors
    /// Returns an error for transport, status, decoding, or empty-result failures.
    pub fn weekly(
        &self,
        area: Area,
        aoi: &str,
        start: Date,
        end: Date,
    ) -> Result<Vec<Week>, Error> {
        let url = self.url(area, aoi, start, end);
        let body = self.get(&url)?;
        let weeks = parse_weeks(&body)?;
        if weeks.is_empty() {
            return Err(Error::NoData {
                area: aoi.to_owned(),
            });
        }
        Ok(weeks)
    }
    fn get(&self, url: &str) -> Result<String, Error> {
        let mut last = None;
        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(400 * u64::from(attempt)));
            }
            match self
                .http
                .get(url)
                .header("Accept", "application/json")
                .send()
            {
                Ok(response) if response.status().is_success() => {
                    return response.text().map_err(Error::Http)
                }
                Ok(response) => {
                    let status = response.status();
                    last = Some(Error::Status {
                        url: url.to_owned(),
                        status: status.as_u16(),
                    });
                    if !(status.as_u16() == 429 || status.is_server_error()) {
                        break;
                    }
                }
                Err(error) => last = Some(Error::Http(error)),
            }
        }
        Err(last.unwrap_or(Error::NoData {
            area: url.to_owned(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(date: &str, d2: f64) -> String {
        format!(
            r#"{{"mapDate":"{date}T00:00:00","none":0.0,"d0":100.0,"d1":50.0,"d2":{d2},"d3":0.0,"d4":0.0}}"#
        )
    }

    #[test]
    fn dates_round_trip_and_shift() {
        let date = Date::parse("2026-11-23T00:00:00").expect("date parses");
        assert_eq!(date.to_string(), "2026-11-23");
        assert_eq!(date.plus_days(-364).to_string(), "2025-11-24");
        assert_eq!(
            Date::parse("2024-02-29")
                .expect("leap")
                .plus_days(1)
                .to_string(),
            "2024-03-01"
        );
        assert!(Date::parse("2026-13-01").is_err());
        assert!(Date::parse("nope").is_err());
    }

    #[test]
    fn url_uses_us_date_format_and_cumulative_statistics() {
        let client = Client::with_base_url("http://x/api/").expect("client");
        let url = client.url(
            Area::County,
            "01001",
            Date::parse("2025-01-05").expect("d"),
            Date::parse("2026-11-23").expect("d"),
        );
        assert_eq!(
            url,
            "http://x/api/CountyStatistics/GetDroughtSeverityStatisticsByAreaPercent?aoi=01001&startdate=1/5/2025&enddate=11/23/2026&statisticsType=1"
        );
    }

    #[test]
    fn summary_compares_against_four_weeks_and_a_year_back() {
        let latest = Date::parse("2026-11-17").expect("d");
        let mut rows = Vec::new();
        for n in 0..60 {
            let d2 = if n == 0 {
                30.0
            } else if n == 4 {
                10.0
            } else if n == 52 {
                5.0
            } else if n < 20 {
                1.0
            } else {
                0.0
            };
            rows.push(row(&latest.plus_days(-7 * n).to_string(), d2));
        }
        let weeks = parse_weeks(&format!("[{}]", rows.join(","))).expect("parses");
        assert_eq!(weeks[0].map_date, latest);
        let summary = summarize(&weeks).expect("summary");
        assert_eq!(summary.latest.d2, 30.0);
        assert_eq!(summary.d2_change_4w, Some(20.0));
        assert_eq!(summary.d2_change_1y, Some(25.0));
        // weeks 0..20 plus week 52 is outside the 52-week window (n 0..=51).
        assert_eq!(summary.weeks_d2_plus_1y, 20);
    }

    #[test]
    fn missing_comparison_weeks_are_none_not_zero() {
        let weeks = parse_weeks(&format!("[{}]", row("2026-11-17", 3.0))).expect("parses");
        let summary = summarize(&weeks).expect("summary");
        assert_eq!(summary.d2_change_4w, None);
        assert_eq!(summary.d2_change_1y, None);
        assert_eq!(summary.weeks_d2_plus_1y, 1);
        assert!(summarize(&[]).is_none());
    }

    #[test]
    fn malformed_bodies_are_decode_errors() {
        assert!(matches!(parse_weeks("{}"), Err(Error::Decode(_))));
        assert!(matches!(
            parse_weeks(r#"[{"mapDate":"2026-01-06"}]"#),
            Err(Error::Decode(_))
        ));
    }
}
