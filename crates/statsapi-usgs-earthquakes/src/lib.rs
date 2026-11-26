//! Blocking client for the USGS Earthquake Catalog (FDSN event web service).
//!
//! Queries go straight to the live API; nothing is persisted.

use reqwest::StatusCode;
use serde_json::Value;
use std::time::Duration;

const BASE: &str = "https://earthquake.usgs.gov/fdsnws/event/1";
const MAX_RETRIES: u32 = 3;

/// Errors returned while querying the catalog.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The HTTP request failed.
    Http(reqwest::Error),
    /// USGS returned a non-success status.
    Status { url: String, status: u16 },
    /// The response body was not the expected shape.
    Decode(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(error) => write!(f, "USGS request failed: {error}"),
            Self::Status { url, status } => write!(f, "USGS returned HTTP {status} for {url}"),
            Self::Decode(message) => write!(f, "USGS response is invalid: {message}"),
        }
    }
}
impl std::error::Error for Error {}

/// The area a query covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Area {
    /// Great-circle radius around a point.
    Radius { lat: f64, lon: f64, km: f64 },
    /// Rectangle `[west, south, east, north]` in degrees.
    Bbox {
        west: f64,
        south: f64,
        east: f64,
        north: f64,
    },
}

/// One catalog query: an area, a UTC date window and a magnitude floor.
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub area: Area,
    /// Inclusive start date, `YYYY-MM-DD`.
    pub start: String,
    pub min_magnitude: Option<f64>,
}

/// The most recent or strongest event of a query.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub magnitude: f64,
    /// USGS display title, e.g. `M 4.8 - 2 km SW of Whitehouse Station, New Jersey`.
    pub title: String,
    /// Event time, milliseconds since the Unix epoch.
    pub time_ms: i64,
    pub url: String,
}

/// Which event a single-row query returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Magnitude,
    Time,
}

/// Blocking USGS catalog client.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
}
impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Creates a client with a descriptive user agent and a 60 s timeout.
    #[must_use]
    pub fn new() -> Self {
        let http = reqwest::blocking::Client::builder()
            .user_agent("statsapi-usgs-earthquakes/0.1 (keyless earthquake reader)")
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_default();
        Self { http }
    }

    /// Counts events matching the query.
    ///
    /// # Errors
    /// Returns an error for HTTP failures or a non-integer body.
    pub fn count(&self, query: &Query, end: &str) -> Result<(u64, String), Error> {
        let url = url("count", query, end, &[]);
        let body = self.get(&url)?;
        let count = body
            .trim()
            .parse()
            .map_err(|_| Error::Decode(format!("count body {body:?}")))?;
        Ok((count, url))
    }

    /// Returns the single strongest or latest event, or `None` when none match.
    ///
    /// # Errors
    /// Returns an error for HTTP failures or a malformed event.
    pub fn top_event(
        &self,
        query: &Query,
        end: &str,
        order: Order,
    ) -> Result<(Option<Event>, String), Error> {
        let order = match order {
            Order::Magnitude => "magnitude",
            Order::Time => "time",
        };
        let url = url(
            "query",
            query,
            end,
            &[("format", "geojson"), ("orderby", order), ("limit", "1")],
        );
        let body = self.get(&url)?;
        let json: Value = serde_json::from_str(&body).map_err(|e| Error::Decode(e.to_string()))?;
        Ok((parse_first_event(&json)?, url))
    }

    fn get(&self, url: &str) -> Result<String, Error> {
        for attempt in 0..=MAX_RETRIES {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    return response.text().map_err(Error::Http);
                }
                Ok(response) => {
                    let status = response.status();
                    // 204 is "no matching events" for count-like queries; 400 is a caller bug.
                    if !(status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                        || attempt == MAX_RETRIES
                    {
                        return Err(Error::Status {
                            url: url.to_owned(),
                            status: status.as_u16(),
                        });
                    }
                }
                Err(error) if attempt == MAX_RETRIES => return Err(Error::Http(error)),
                Err(_) => {}
            }
            std::thread::sleep(Duration::from_millis(500 * (1 << attempt)));
        }
        Err(Error::Decode("retry loop exhausted".into()))
    }
}

/// Builds a catalog URL for `endpoint` (`count` or `query`).
#[must_use]
pub fn url(endpoint: &str, query: &Query, end: &str, extra: &[(&str, &str)]) -> String {
    let mut params: Vec<String> = extra.iter().map(|(k, v)| format!("{k}={v}")).collect();
    match query.area {
        Area::Radius { lat, lon, km } => params.extend([
            format!("latitude={lat:.4}"),
            format!("longitude={lon:.4}"),
            format!("maxradiuskm={km}"),
        ]),
        Area::Bbox {
            west,
            south,
            east,
            north,
        } => params.extend([
            format!("minlatitude={south:.4}"),
            format!("maxlatitude={north:.4}"),
            format!("minlongitude={west:.4}"),
            format!("maxlongitude={east:.4}"),
        ]),
    }
    params.push(format!("starttime={}", query.start));
    params.push(format!("endtime={end}"));
    if let Some(min) = query.min_magnitude {
        params.push(format!("minmagnitude={min}"));
    }
    format!("{BASE}/{endpoint}?{}", params.join("&"))
}

/// Parses the first feature of a `GeoJSON` event collection.
///
/// # Errors
/// Returns an error when a feature lacks magnitude, title or time.
pub fn parse_first_event(json: &Value) -> Result<Option<Event>, Error> {
    let Some(feature) = json["features"].as_array().and_then(|f| f.first()) else {
        return Ok(None);
    };
    let p = &feature["properties"];
    let missing = |field: &str| Error::Decode(format!("event lacks {field}"));
    Ok(Some(Event {
        magnitude: p["mag"].as_f64().ok_or_else(|| missing("mag"))?,
        title: p["title"]
            .as_str()
            .ok_or_else(|| missing("title"))?
            .to_owned(),
        time_ms: p["time"].as_i64().ok_or_else(|| missing("time"))?,
        url: p["url"].as_str().unwrap_or_default().to_owned(),
    }))
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Returns the date `days` before `date` (`YYYY-MM-DD`), or `None` if unparsable.
#[must_use]
pub fn days_before(date: &str, days: i64) -> Option<String> {
    let mut parts = date.split('-');
    let y = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || parts.next().is_some() {
        return None;
    }
    let (y, m, d) = civil_from_days(days_from_civil(y, m, d) - days);
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

/// Formats an epoch-millisecond timestamp as `YYYY-MM-DD` (UTC).
#[must_use]
pub fn date_of(time_ms: i64) -> String {
    let (y, m, d) = civil_from_days(time_ms.div_euclid(86_400_000));
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_math_crosses_leap_days_and_years() {
        assert_eq!(days_before("2026-11-23", 30).as_deref(), Some("2026-10-24"));
        assert_eq!(days_before("2024-03-01", 1).as_deref(), Some("2024-02-29"));
        assert_eq!(days_before("2026-01-05", 10).as_deref(), Some("2025-12-26"));
        assert_eq!(days_before("garbage", 1), None);
        assert_eq!(date_of(1_712_327_000_045), "2024-04-05");
    }

    #[test]
    fn urls_carry_area_window_and_magnitude() {
        let q = Query {
            area: Area::Radius {
                lat: 40.5,
                lon: -74.3,
                km: 100.0,
            },
            start: "2025-11-23".into(),
            min_magnitude: Some(3.0),
        };
        let u = url("count", &q, "2026-11-23", &[]);
        assert_eq!(
            u,
            "https://earthquake.usgs.gov/fdsnws/event/1/count?latitude=40.5000&longitude=-74.3000&maxradiuskm=100&starttime=2025-11-23&endtime=2026-11-23&minmagnitude=3"
        );
        let b = Query {
            area: Area::Bbox {
                west: 1.0,
                south: 2.0,
                east: 3.0,
                north: 4.0,
            },
            start: "2025-11-23".into(),
            min_magnitude: None,
        };
        assert!(url("query", &b, "2026-11-23", &[("limit", "1")]).contains(
            "limit=1&minlatitude=2.0000&maxlatitude=4.0000&minlongitude=1.0000&maxlongitude=3.0000"
        ));
    }

    #[test]
    fn first_event_parses_from_recorded_response() {
        let json: Value =
            serde_json::from_str(include_str!("../tests/fixtures/top_event.json")).unwrap();
        let event = parse_first_event(&json).unwrap().unwrap();
        assert!((event.magnitude - 4.8).abs() < 1e-9);
        assert!(event.title.starts_with("M 4.8"));
        assert_eq!(date_of(event.time_ms), "2024-04-05");
        assert_eq!(
            parse_first_event(&serde_json::json!({"features": []})).unwrap(),
            None
        );
    }
}
