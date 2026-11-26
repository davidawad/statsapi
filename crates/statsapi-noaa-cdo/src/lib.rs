//! A reusable client for the keyless NOAA NCEI Access Data Service.
//!
//! It finds the nearest GHCN-Daily stations to a point (NCEI search service),
//! reads daily summaries (`daily-summaries`) and 1991-2020 daily normals
//! (`normals-daily`). Every call is a live request; nothing is cached or
//! written to disk.

use chrono::NaiveDate;
use reqwest::blocking::Client as HttpClient;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

const DATA_URL: &str = "https://www.ncei.noaa.gov/access/services/data/v1";
const SEARCH_URL: &str = "https://www.ncei.noaa.gov/access/services/search/v1/data";
const EARTH_RADIUS_KM: f64 = 6371.0088;
const USER_AGENT: &str = "statsapi-noaa-cdo/0.1 (keyless place climate reader)";

/// A client error that can be displayed and used as an error source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self(message.to_owned())
    }
}

/// A GHCN-Daily station found near a point.
#[derive(Clone, Debug, PartialEq)]
pub struct Station {
    /// GHCN-Daily station id, for example `USW00014734`.
    pub id: String,
    /// Longitude in degrees east.
    pub lon: f64,
    /// Latitude in degrees north.
    pub lat: f64,
    /// Last date the daily-summaries file covers.
    pub end_date: Option<NaiveDate>,
    /// Great-circle distance from the query point in kilometres.
    pub distance_km: f64,
}

/// One station-day of daily summaries, in NCEI "standard" units (inches, °F).
#[derive(Clone, Debug, PartialEq)]
pub struct Day {
    /// Observation date.
    pub date: NaiveDate,
    /// Numeric data types present that day (`PRCP`, `TMAX`, `TMIN`, `TAVG`).
    pub values: BTreeMap<String, f64>,
}

/// Daily climate normals for one calendar day (1991-2020).
#[derive(Clone, Debug, PartialEq)]
pub struct Normal {
    /// Normal mean temperature in °F.
    pub tavg: Option<f64>,
    /// Year-to-date normal precipitation in inches.
    pub ytd_prcp: Option<f64>,
}

/// Great-circle distance in kilometres.
#[must_use]
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dphi = (lat2 - lat1).to_radians();
    let dlambda = (lon2 - lon1).to_radians();
    let a = (dphi / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dlambda / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * a.sqrt().asin()
}

/// Parses a search-service response into stations sorted nearest first.
///
/// # Errors
/// Returns an error when the body is not the search-service JSON shape.
pub fn parse_stations(body: &str, lat: f64, lon: f64) -> Result<Vec<Station>, Error> {
    let payload: Value = serde_json::from_str(body).map_err(|e| Error(e.to_string()))?;
    let results = payload["results"].as_array().ok_or_else(|| {
        Error(format!(
            "search response has no results: {}",
            payload["errorMessage"]
        ))
    })?;
    let mut stations: Vec<Station> = results
        .iter()
        .filter_map(|result| {
            let id = result["filePath"]
                .as_str()?
                .rsplit('/')
                .next()?
                .strip_suffix(".csv")?
                .to_owned();
            let point = result["boundingPoints"].get(0)?["coordinates"].as_array()?;
            let (slon, slat) = (point.first()?.as_f64()?, point.get(1)?.as_f64()?);
            let end_date = result["endDate"]
                .as_str()
                .and_then(|d| NaiveDate::parse_from_str(d.get(..10)?, "%Y-%m-%d").ok());
            Some(Station {
                id,
                lon: slon,
                lat: slat,
                end_date,
                distance_km: haversine_km(lat, lon, slat, slon),
            })
        })
        .collect();
    stations.sort_by(|a, b| a.distance_km.total_cmp(&b.distance_km));
    Ok(stations)
}

/// Parses a `daily-summaries` JSON response.
///
/// # Errors
/// Returns an error when the body is not a JSON array of rows.
pub fn parse_days(body: &str) -> Result<Vec<Day>, Error> {
    let rows: Vec<Value> = serde_json::from_str(body).map_err(|e| Error(e.to_string()))?;
    let mut days: Vec<Day> = rows
        .iter()
        .filter_map(|row| {
            let date = NaiveDate::parse_from_str(row["DATE"].as_str()?, "%Y-%m-%d").ok()?;
            let values = row
                .as_object()?
                .iter()
                .filter(|(key, _)| matches!(key.as_str(), "PRCP" | "TMAX" | "TMIN" | "TAVG"))
                .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.parse().ok()?)))
                .collect();
            Some(Day { date, values })
        })
        .collect();
    days.sort_by_key(|day| day.date);
    Ok(days)
}

/// Parses a `normals-daily` JSON response into normals keyed by `MM-DD`.
///
/// # Errors
/// Returns an error when the body is not a JSON array of rows.
pub fn parse_normals(body: &str) -> Result<BTreeMap<String, Normal>, Error> {
    let rows: Vec<Value> = serde_json::from_str(body).map_err(|e| Error(e.to_string()))?;
    let number = |row: &Value, key: &str| {
        row[key]
            .as_str()
            .and_then(|text| text.parse::<f64>().ok())
            .filter(|n| *n > -6000.0)
    };
    Ok(rows
        .iter()
        .filter_map(|row| {
            Some((
                row["DATE"].as_str()?.to_owned(),
                Normal {
                    tavg: number(row, "DLY-TAVG-NORMAL"),
                    ytd_prcp: number(row, "YTD-PRCP-NORMAL"),
                },
            ))
        })
        .collect())
}

/// The blocking NCEI client.
#[derive(Clone, Debug)]
pub struct Client {
    http: HttpClient,
}

impl Client {
    /// Creates a client with a descriptive User-Agent and a 60 s timeout.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn new() -> Result<Self, Error> {
        let http = HttpClient::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| Error(e.to_string()))?;
        Ok(Self { http })
    }

    fn get(&self, url: &str, query: &[(&str, String)]) -> Result<String, Error> {
        let response = self
            .http
            .get(url)
            .query(query)
            .send()
            .map_err(|e| Error(e.to_string()))?;
        let status = response.status();
        let body = response.text().map_err(|e| Error(e.to_string()))?;
        if status.is_success() {
            Ok(body)
        } else {
            Err(Error(format!("NCEI answered {status}: {}", body.trim())))
        }
    }

    /// Stations within `half_deg` degrees of the point that report precipitation
    /// and maximum temperature over the date range, nearest first.
    ///
    /// # Errors
    /// Returns an error on a failed request or an unexpected response.
    pub fn stations_near(
        &self,
        lat: f64,
        lon: f64,
        half_deg: f64,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<Station>, Error> {
        let bbox = format!(
            "{},{},{},{}",
            lat + half_deg,
            lon - half_deg,
            lat - half_deg,
            lon + half_deg
        );
        let body = self.get(
            SEARCH_URL,
            &[
                ("dataset", "daily-summaries".into()),
                ("bbox", bbox),
                ("dataTypes", "PRCP,TMAX".into()),
                ("startDate", start.to_string()),
                ("endDate", end.to_string()),
                ("limit", "1000".into()),
            ],
        )?;
        parse_stations(&body, lat, lon)
    }

    /// Daily summaries for one station and date range.
    ///
    /// # Errors
    /// Returns an error on a failed request or an unexpected response.
    pub fn daily_summaries(
        &self,
        station: &str,
        start: NaiveDate,
        end: NaiveDate,
        data_types: &str,
    ) -> Result<Vec<Day>, Error> {
        let body = self.get(
            DATA_URL,
            &[
                ("dataset", "daily-summaries".into()),
                ("stations", station.into()),
                ("startDate", start.to_string()),
                ("endDate", end.to_string()),
                ("dataTypes", data_types.into()),
                ("units", "standard".into()),
                ("format", "json".into()),
            ],
        )?;
        if body.trim().is_empty() {
            return Ok(Vec::new());
        }
        parse_days(&body)
    }

    /// All 366 daily normals (1991-2020) for a station, keyed by `MM-DD`.
    /// Empty when the station has no normals.
    ///
    /// # Errors
    /// Returns an error on a failed request or an unexpected response.
    pub fn daily_normals(&self, station: &str) -> Result<BTreeMap<String, Normal>, Error> {
        let body = self.get(
            DATA_URL,
            &[
                ("dataset", "normals-daily".into()),
                ("stations", station.into()),
                ("startDate", "2012-01-01".into()),
                ("endDate", "2012-12-31".into()),
                ("dataTypes", "DLY-TAVG-NORMAL,YTD-PRCP-NORMAL".into()),
                ("units", "standard".into()),
                ("format", "json".into()),
            ],
        )?;
        if body.trim().is_empty() {
            return Ok(BTreeMap::new());
        }
        parse_normals(&body)
    }

    /// The request URL for citing a station's daily summaries.
    #[must_use]
    pub fn citation_url(station: &str) -> String {
        format!("{DATA_URL}?dataset=daily-summaries&stations={station}")
    }
}
