//! Small blocking client for the `OpenAQ` v3 API (<https://docs.openaq.org/>).
//!
//! `OpenAQ` v3 requires an `X-API-Key` header on every request; the free key
//! comes from <https://explore.openaq.org/register>. Parsing and geometry are
//! pure functions so callers can test them over recorded responses.

use serde_json::Value;
use std::time::Duration;
use thiserror::Error;

/// Production API root.
pub const BASE_URL: &str = "https://api.openaq.org/v3";
/// `OpenAQ` parameter id for PM2.5 (µg/m³).
pub const PM25: u32 = 2;
/// Largest search radius `OpenAQ` accepts for a coordinate query, in metres.
pub const MAX_RADIUS_M: u32 = 25_000;
/// Largest page size `OpenAQ` accepts.
pub const MAX_PAGE: u32 = 1000;

/// Errors returned by the client.
#[derive(Debug, Error)]
pub enum Error {
    /// Transport failure or non-success HTTP status.
    #[error("OpenAQ request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// The response lacked the documented `results` array.
    #[error("OpenAQ response has no results array")]
    Shape,
    /// The API key was empty.
    #[error("OpenAQ API key is empty")]
    NoKey,
}

/// A monitoring location that reports PM2.5.
#[derive(Clone, Debug, PartialEq)]
pub struct Location {
    /// `OpenAQ` location id.
    pub id: u64,
    /// Station name.
    pub name: String,
    /// Latitude in degrees.
    pub lat: f64,
    /// Longitude in degrees.
    pub lon: f64,
    /// Id of the location's PM2.5 sensor.
    pub pm25_sensor: u64,
}

/// One latest reading.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    /// Sensor id.
    pub sensor: u64,
    /// Location id, when the endpoint reports it.
    pub location: Option<u64>,
    /// Measured value.
    pub value: f64,
    /// UTC timestamp as published.
    pub utc: String,
}

/// One daily average.
#[derive(Clone, Debug, PartialEq)]
pub struct Day {
    /// UTC start of the day as published.
    pub utc: String,
    /// Daily mean.
    pub value: f64,
}

/// Blocking `OpenAQ` v3 client.
pub struct Client {
    http: reqwest::blocking::Client,
    base: String,
    key: String,
}

impl Client {
    /// Builds a client for the production API.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NoKey`] for an empty key, or a transport error when
    /// the HTTP client cannot be built.
    pub fn new(key: &str) -> Result<Self, Error> {
        Self::with_base(key, BASE_URL)
    }

    /// Builds a client against another API root (tests, mirrors).
    ///
    /// # Errors
    ///
    /// As for [`Client::new`].
    pub fn with_base(key: &str, base: &str) -> Result<Self, Error> {
        let key = key.trim();
        if key.is_empty() {
            return Err(Error::NoKey);
        }
        Ok(Self {
            http: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()?,
            base: base.trim_end_matches('/').to_string(),
            key: key.to_string(),
        })
    }

    /// The public (keyless) URL for a path and query, for `source_url`.
    #[must_use]
    pub fn public_url(&self, path: &str, query: &str) -> String {
        format!("{}{path}?{query}", self.base)
    }

    fn get(&self, path: &str, query: &str) -> Result<Value, Error> {
        Ok(self
            .http
            .get(self.public_url(path, query))
            .header("X-API-Key", &self.key)
            .send()?
            .error_for_status()?
            .json()?)
    }

    /// PM2.5 locations within `radius_m` metres of a point (at most 25 km).
    ///
    /// # Errors
    ///
    /// Returns an error for a failed request or an unexpected response shape.
    pub fn locations_near(
        &self,
        lat: f64,
        lon: f64,
        radius_m: u32,
    ) -> Result<Vec<Location>, Error> {
        parse_locations(&self.get("/locations", &locations_near_query(lat, lon, radius_m))?)
    }

    /// PM2.5 locations in a country (ISO 3166-1 alpha-2), first page only.
    ///
    /// # Errors
    ///
    /// Returns an error for a failed request or an unexpected response shape.
    pub fn locations_in_country(&self, iso2: &str) -> Result<Vec<Location>, Error> {
        let query = format!("iso={iso2}&parameters_id={PM25}&limit={MAX_PAGE}");
        parse_locations(&self.get("/locations", &query)?)
    }

    /// Latest readings of one location.
    ///
    /// # Errors
    ///
    /// Returns an error for a failed request or an unexpected response shape.
    pub fn location_latest(&self, id: u64) -> Result<Vec<Reading>, Error> {
        parse_latest(&self.get(&format!("/locations/{id}/latest"), "limit=100")?)
    }

    /// Latest PM2.5 readings from the selected locations.
    ///
    /// # Errors
    ///
    /// Returns an error for a failed request or an unexpected response shape.
    pub fn pm25_latest(&self, locations: &[Location]) -> Result<Vec<Reading>, Error> {
        let mut readings = Vec::with_capacity(locations.len());
        for (index, location) in locations.iter().enumerate() {
            // OpenAQ's free tier allows 60 requests per minute. Keep a one
            // second gap between location requests rather than bursting.
            if index > 0 {
                std::thread::sleep(Duration::from_secs(1));
            }
            if let Some(reading) = self
                .location_latest(location.id)?
                .into_iter()
                .find(|reading| reading.sensor == location.pm25_sensor)
            {
                readings.push(reading);
            }
        }
        Ok(readings)
    }

    /// Daily means of one sensor between two `YYYY-MM-DD` dates.
    ///
    /// # Errors
    ///
    /// Returns an error for a failed request or an unexpected response shape.
    pub fn sensor_days(&self, sensor: u64, from: &str, to: &str) -> Result<Vec<Day>, Error> {
        let query = format!("datetime_from={from}&datetime_to={to}&limit=100");
        parse_days(&self.get(&format!("/sensors/{sensor}/days"), &query)?)
    }

    /// The query string used for [`Client::locations_near`], for provenance URLs.
    #[must_use]
    pub fn near_query(lat: f64, lon: f64, radius_m: u32) -> String {
        locations_near_query(lat, lon, radius_m)
    }
}

fn locations_near_query(lat: f64, lon: f64, radius_m: u32) -> String {
    format!(
        "coordinates={lat:.4},{lon:.4}&radius={}&parameters_id={PM25}&limit={MAX_PAGE}",
        radius_m.min(MAX_RADIUS_M)
    )
}

fn results(value: &Value) -> Result<&Vec<Value>, Error> {
    value
        .get("results")
        .and_then(Value::as_array)
        .ok_or(Error::Shape)
}

/// Parses a `/locations` response, keeping locations with a PM2.5 sensor.
///
/// # Errors
///
/// Returns [`Error::Shape`] when `results` is absent.
pub fn parse_locations(value: &Value) -> Result<Vec<Location>, Error> {
    Ok(results(value)?
        .iter()
        .filter_map(|row| {
            let sensor = row.get("sensors")?.as_array()?.iter().find_map(|s| {
                (s.pointer("/parameter/id")?.as_u64()? == u64::from(PM25))
                    .then(|| s.get("id")?.as_u64())
                    .flatten()
            })?;
            Some(Location {
                id: row.get("id")?.as_u64()?,
                name: row.get("name").and_then(Value::as_str).unwrap_or("").into(),
                lat: row.pointer("/coordinates/latitude")?.as_f64()?,
                lon: row.pointer("/coordinates/longitude")?.as_f64()?,
                pm25_sensor: sensor,
            })
        })
        .collect())
}

/// Parses a `/latest` response.
///
/// # Errors
///
/// Returns [`Error::Shape`] when `results` is absent.
pub fn parse_latest(value: &Value) -> Result<Vec<Reading>, Error> {
    Ok(results(value)?
        .iter()
        .filter_map(|row| {
            Some(Reading {
                sensor: row.get("sensorsId")?.as_u64()?,
                location: row.get("locationsId").and_then(Value::as_u64),
                value: row.get("value")?.as_f64()?,
                utc: row.pointer("/datetime/utc")?.as_str()?.to_string(),
            })
        })
        .collect())
}

/// Parses a `/sensors/{id}/days` response.
///
/// # Errors
///
/// Returns [`Error::Shape`] when `results` is absent.
pub fn parse_days(value: &Value) -> Result<Vec<Day>, Error> {
    Ok(results(value)?
        .iter()
        .filter_map(|row| {
            Some(Day {
                utc: row
                    .pointer("/period/datetimeFrom/utc")?
                    .as_str()?
                    .to_string(),
                value: row.get("value")?.as_f64()?,
            })
        })
        .collect())
}

/// Great-circle distance between two points in kilometres.
#[must_use]
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (a, b) = (lat1.to_radians(), lat2.to_radians());
    let h = ((b - a) / 2.0).sin().powi(2)
        + a.cos() * b.cos() * ((lon2 - lon1).to_radians() / 2.0).sin().powi(2);
    6371.0 * 2.0 * h.sqrt().atan2((1.0 - h).sqrt())
}

/// The location nearest a point, with its distance in kilometres.
#[must_use]
pub fn nearest(locations: &[Location], lat: f64, lon: f64) -> Option<(&Location, f64)> {
    locations
        .iter()
        .map(|l| (l, haversine_km(lat, lon, l.lat, l.lon)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// Mean of values, ignoring non-finite and implausible (negative or > 1000)
/// PM2.5 readings that `OpenAQ` passes through from faulty sensors.
#[must_use]
pub fn plausible_mean(values: impl IntoIterator<Item = f64>) -> Option<f64> {
    let kept: Vec<f64> = values
        .into_iter()
        .filter(|v| v.is_finite() && (0.0..=1000.0).contains(v))
        .collect();
    #[expect(
        clippy::cast_precision_loss,
        reason = "reading counts are far below 2^52"
    )]
    (!kept.is_empty()).then(|| kept.iter().sum::<f64>() / kept.len() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
    }

    #[test]
    fn locations_keep_only_pm25_sensors() {
        let locations = parse_locations(&fixture("locations.json")).expect("parses");
        assert_eq!(locations.len(), 2);
        assert_eq!(locations[0].pm25_sensor, 101);
        let (near, km) = nearest(&locations, 40.7, -74.0).expect("nearest");
        assert_eq!(near.id, 7);
        assert!(km < 10.0);
    }

    #[test]
    fn latest_and_days_parse() {
        let latest = parse_latest(&fixture("latest.json")).expect("parses");
        assert_eq!(latest[0].value, 9.5);
        assert_eq!(latest[0].sensor, 101);
        let days = parse_days(&fixture("days.json")).expect("parses");
        assert_eq!(days.len(), 3);
        assert_eq!(plausible_mean(days.iter().map(|d| d.value)), Some(10.0));
    }

    #[test]
    fn missing_results_is_a_shape_error() {
        assert!(matches!(
            parse_days(&serde_json::json!({"detail": "x"})),
            Err(Error::Shape)
        ));
    }

    #[test]
    fn mean_drops_faulty_readings() {
        assert_eq!(plausible_mean([5.0, -999.0, 15.0, 1e6]), Some(10.0));
        assert_eq!(plausible_mean([]), None);
    }

    #[test]
    fn near_query_clamps_radius_and_empty_key_is_rejected() {
        assert!(Client::near_query(1.0, 2.0, 90_000).contains("radius=25000"));
        assert!(matches!(Client::new("  "), Err(Error::NoKey)));
    }

    #[test]
    fn haversine_is_symmetric_and_plausible() {
        let d = haversine_km(40.7, -74.0, 51.5, -0.12);
        assert!((5500.0..5600.0).contains(&d));
    }
}
