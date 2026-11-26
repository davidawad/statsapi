//! Blocking client for the `OpenSky` Network live state-vector API.
//!
//! Anonymous access is rate limited (a few hundred credits per day, ten
//! second time resolution); callers should ask once per bounding box. The
//! historical `flights/*` endpoints refuse anonymous requests and are not
//! covered.

use reqwest::blocking::Client as HttpClient;
use serde_json::Value;
use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BASE: &str = "https://opensky-network.org/api";
const KM_PER_DEGREE: f64 = 111.32;

/// A WGS84 bounding box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundingBox {
    min_lat: f64,
    min_lon: f64,
    max_lat: f64,
    max_lon: f64,
}

impl BoundingBox {
    /// Creates a box from south, west, north and east edges in degrees.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::InvalidQuery`] when an edge is out of range or
    /// the box is inverted.
    pub fn new(
        min_lat: f64,
        min_lon: f64,
        max_lat: f64,
        max_lon: f64,
    ) -> Result<Self, ClientError> {
        let lat_ok = |v: f64| (-90.0..=90.0).contains(&v);
        let lon_ok = |v: f64| (-180.0..=180.0).contains(&v);
        if !(lat_ok(min_lat) && lat_ok(max_lat) && lon_ok(min_lon) && lon_ok(max_lon))
            || min_lat >= max_lat
            || min_lon >= max_lon
        {
            return Err(ClientError::InvalidQuery(
                "bounding box edges out of range or inverted".into(),
            ));
        }
        Ok(Self {
            min_lat,
            min_lon,
            max_lat,
            max_lon,
        })
    }

    /// A box reaching `radius_km` from a point, clamped at the poles and
    /// the antimeridian (traffic across the antimeridian is not wrapped).
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::InvalidQuery`] for an invalid point or radius.
    pub fn around(lat: f64, lon: f64, radius_km: f64) -> Result<Self, ClientError> {
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) || radius_km <= 0.0 {
            return Err(ClientError::InvalidQuery("invalid point or radius".into()));
        }
        let dlat = radius_km / KM_PER_DEGREE;
        let cos = lat.to_radians().cos().max(0.01);
        let dlon = radius_km / (KM_PER_DEGREE * cos);
        Self::new(
            (lat - dlat).max(-90.0),
            (lon - dlon).max(-180.0),
            (lat + dlat).min(90.0),
            (lon + dlon).min(180.0),
        )
    }

    /// The `states/all` URL for this box.
    #[must_use]
    pub fn url(&self) -> String {
        format!(
            "{BASE}/states/all?lamin={:.4}&lomin={:.4}&lamax={:.4}&lomax={:.4}",
            self.min_lat, self.min_lon, self.max_lat, self.max_lon
        )
    }
}

/// Aircraft counted in one bounding box at one instant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Snapshot {
    /// Unix time of the state vectors.
    pub time: i64,
    /// Aircraft with a state vector in the box.
    pub total: usize,
    /// Of those, aircraft reporting `on_ground`.
    pub on_ground: usize,
}

impl Snapshot {
    /// Airborne aircraft.
    #[must_use]
    pub fn airborne(&self) -> usize {
        self.total - self.on_ground
    }

    /// Share of aircraft on the ground, as a percentage; `None` when empty.
    #[must_use]
    pub fn on_ground_percent(&self) -> Option<f64> {
        #[expect(clippy::cast_precision_loss, reason = "aircraft counts are small")]
        (self.total > 0).then(|| self.on_ground as f64 * 100.0 / self.total as f64)
    }
}

/// Errors from building or fetching a state-vector response.
#[derive(Debug)]
#[non_exhaustive]
pub enum ClientError {
    /// A bounding box or point is invalid.
    InvalidQuery(String),
    /// An HTTP request failed after retries.
    Http(reqwest::Error),
    /// The response body was not valid JSON.
    Json(serde_json::Error),
    /// The JSON lacked the documented `time` or `states` fields.
    Shape(&'static str),
    /// The anonymous rate limit was hit.
    RateLimited,
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQuery(message) => write!(f, "invalid query: {message}"),
            Self::Http(error) => write!(f, "OpenSky request failed: {error}"),
            Self::Json(error) => write!(f, "invalid OpenSky JSON: {error}"),
            Self::Shape(message) => write!(f, "unexpected OpenSky response: {message}"),
            Self::RateLimited => write!(f, "OpenSky anonymous rate limit reached (HTTP 429)"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::InvalidQuery(_) | Self::Shape(_) | Self::RateLimited => None,
        }
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// A blocking anonymous `OpenSky` client. Nothing is cached or persisted.
#[derive(Clone, Debug)]
pub struct OpenSkyClient {
    http: HttpClient,
    max_retries: usize,
}

impl OpenSkyClient {
    /// Creates a client with a 30 second timeout and two retries.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be initialized.
    pub fn new() -> Result<Self, ClientError> {
        Ok(Self {
            http: HttpClient::builder()
                .user_agent(concat!("statsapi-opensky/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(30))
                .build()?,
            max_retries: 2,
        })
    }

    /// Sets the number of retries after the initial attempt.
    #[must_use]
    pub fn with_max_retries(mut self, retries: usize) -> Self {
        self.max_retries = retries;
        self
    }

    /// Fetches live state vectors in `bbox` and returns the URL and counts.
    ///
    /// # Errors
    ///
    /// Returns an error for network failures, HTTP errors (including the
    /// anonymous rate limit), or an unexpected response shape.
    pub fn snapshot(&self, bbox: &BoundingBox) -> Result<(String, Snapshot), ClientError> {
        let url = bbox.url();
        let body = self.get_with_retry(&url)?;
        let snapshot = parse_snapshot(&serde_json::from_slice(&body)?)?;
        Ok((url, snapshot))
    }

    fn get_with_retry(&self, url: &str) -> Result<Vec<u8>, ClientError> {
        let mut attempt = 0;
        loop {
            let outcome = self.http.get(url).send();
            match outcome {
                Ok(response) if response.status().is_success() => {
                    return Ok(response.bytes()?.to_vec());
                }
                Ok(response) => {
                    let status = response.status().as_u16();
                    if !retryable_status(status) || attempt >= self.max_retries {
                        return Err(if status == 429 {
                            ClientError::RateLimited
                        } else {
                            response
                                .error_for_status()
                                .err()
                                .map_or(ClientError::Shape("non-success status"), ClientError::Http)
                        });
                    }
                }
                Err(error) if attempt >= self.max_retries => return Err(error.into()),
                Err(_) => {}
            }
            jittered_delay(attempt);
            attempt += 1;
        }
    }
}

/// Counts aircraft in an `states/all` document.
///
/// # Errors
///
/// Returns [`ClientError::Shape`] when `time` or `states` is missing.
pub fn parse_snapshot(payload: &Value) -> Result<Snapshot, ClientError> {
    let time = payload
        .get("time")
        .and_then(Value::as_i64)
        .ok_or(ClientError::Shape("missing time"))?;
    // OpenSky returns `"states": null` when the box is empty.
    let states = match payload.get("states") {
        Some(Value::Array(rows)) => rows.as_slice(),
        Some(Value::Null) => &[],
        _ => return Err(ClientError::Shape("missing states")),
    };
    let on_ground = states
        .iter()
        .filter(|row| row.get(8).and_then(Value::as_bool) == Some(true))
        .count();
    Ok(Snapshot {
        time,
        total: states.len(),
        on_ground,
    })
}

fn retryable_status(status: u16) -> bool {
    status == 429 || (500..=599).contains(&status)
}

fn jittered_delay(attempt: usize) {
    let base_ms = 500_u64.saturating_mul(1_u64 << attempt.min(5));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    std::thread::sleep(Duration::from_millis(
        base_ms / 2 + u64::from(nanos) % base_ms,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/states.json")).expect("fixture JSON")
    }

    #[test]
    fn counts_total_and_ground_rows() {
        let snapshot = parse_snapshot(&fixture()).expect("snapshot");
        assert_eq!(snapshot.time, 1_700_000_000);
        assert_eq!(snapshot.total, 4);
        assert_eq!(snapshot.on_ground, 1);
        assert_eq!(snapshot.airborne(), 3);
        assert_eq!(snapshot.on_ground_percent(), Some(25.0));
    }

    #[test]
    fn null_states_is_an_empty_box() {
        let snapshot = parse_snapshot(&serde_json::json!({"time": 5, "states": null}))
            .expect("empty snapshot");
        assert_eq!(snapshot.total, 0);
        assert_eq!(snapshot.on_ground_percent(), None);
        assert!(parse_snapshot(&serde_json::json!({"states": []})).is_err());
    }

    #[test]
    fn box_around_a_point_spans_about_the_radius() {
        let bbox = BoundingBox::around(40.0, -74.0, 100.0).expect("box");
        assert!((bbox.max_lat - bbox.min_lat - 1.7966).abs() < 0.01);
        assert!(bbox.url().contains("lamin=39.1017"));
        assert!(BoundingBox::around(91.0, 0.0, 10.0).is_err());
        assert!(BoundingBox::new(10.0, 0.0, 5.0, 1.0).is_err());
    }

    #[test]
    #[ignore = "live OpenSky anonymous state-vector smoke test"]
    fn live_snapshot_counts_aircraft_near_new_york() {
        let bbox = BoundingBox::around(40.7128, -74.006, 100.0).expect("box");
        let (_, snapshot) = OpenSkyClient::new()
            .expect("HTTP client builds")
            .snapshot(&bbox)
            .expect("OpenSky response");
        assert!(snapshot.total > 0, "no aircraft over New York");
    }
}
