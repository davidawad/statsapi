//! Blocking client for EPA Envirofacts TRI tables and the Superfund NPL layer.
// Pound totals and site counts stay far below f64's exact integer range.
#![allow(clippy::cast_precision_loss)]

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Envirofacts REST root (keyless).
pub const EFSERVICE: &str = "https://data.epa.gov/efservice";
/// EPA's public NPL-with-status `ArcGIS` feature layer.
pub const NPL_LAYER: &str = "https://services.arcgis.com/cJ9YHowT8TU7DUyn/ArcGIS/rest/services/Superfund_National_Priorities_List_(NPL)_Sites_with_Status_Information/FeatureServer/0";

const MAX_RETRIES: u32 = 3;
const PAGE_ROWS: usize = 2000;
const EARTH_RADIUS_KM: f64 = 6371.0;

/// Errors returned by the client.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The HTTP request failed.
    Http(reqwest::Error),
    /// The service returned a non-success status.
    Status { url: String, status: u16 },
    /// The response was not the expected JSON shape.
    Decode(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(error) => write!(f, "EPA request failed: {error}"),
            Self::Status { url, status } => write!(f, "EPA returned HTTP {status} for {url}"),
            Self::Decode(message) => write!(f, "EPA response is invalid: {message}"),
        }
    }
}
impl std::error::Error for Error {}

/// A TRI geography the Envirofacts `tri_facility` table can filter by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriArea {
    /// A five-digit ZIP code (the ZCTA's twin).
    Zip(String),
    /// A five-digit state+county FIPS code.
    County(String),
}

impl TriArea {
    fn filter(&self) -> String {
        match self {
            Self::Zip(zip) => format!("zip_code/{zip}"),
            Self::County(fips) => format!("state_county_fips_code/{fips}"),
        }
    }
}

/// One Superfund NPL site with its distance from the query point.
#[derive(Debug, Clone, PartialEq)]
pub struct NplSite {
    pub name: String,
    pub epa_id: String,
    pub status: String,
    pub city: String,
    pub state: String,
    pub lat: f64,
    pub lon: f64,
    pub distance_km: f64,
}

/// Facility count and total on-site releases for one reporting year.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriSummary {
    pub facilities: usize,
    pub onsite_release_lbs: f64,
}

/// Blocking client; every call is a live request (nothing is persisted).
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
    /// Creates a client with a 120-second request timeout.
    #[must_use]
    pub fn new() -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .unwrap_or_default();
        Self { http }
    }

    /// Fetches a JSON document, retrying transport errors, 429 and 5xx with jitter.
    ///
    /// # Errors
    /// Returns an error for HTTP failures or a body that is not JSON.
    pub fn get_json(&self, url: &str) -> Result<Value, Error> {
        let mut attempt = 0;
        loop {
            match self.http.get(url).send() {
                Ok(response) => {
                    let status = response.status();
                    if (status.is_server_error() || status.as_u16() == 429) && attempt < MAX_RETRIES
                    {
                        attempt += 1;
                        std::thread::sleep(backoff(url, attempt));
                        continue;
                    }
                    if !status.is_success() {
                        return Err(Error::Status {
                            url: url.to_string(),
                            status: status.as_u16(),
                        });
                    }
                    return response.json().map_err(Error::Http);
                }
                Err(_) if attempt < MAX_RETRIES => {
                    attempt += 1;
                    std::thread::sleep(backoff(url, attempt));
                }
                Err(error) => return Err(Error::Http(error)),
            }
        }
    }

    /// Returns the newest TRI reporting year at or before `from_year` that has forms.
    ///
    /// # Errors
    /// Returns an error when no year in the four-year window has data, or a request fails.
    pub fn latest_tri_year(&self, from_year: i32) -> Result<i32, Error> {
        for year in (from_year - 4..=from_year).rev() {
            let url = format!("{EFSERVICE}/tri_reporting_form/reporting_year/{year}/count/JSON");
            if parse_count(&self.get_json(&url)?) > 0 {
                return Ok(year);
            }
        }
        Err(Error::Decode(format!(
            "no TRI reporting forms for {}-{from_year}",
            from_year - 4
        )))
    }

    /// URL of the joined facility, form and release-quantity query for an area and year.
    #[must_use]
    pub fn tri_url(area: &TriArea, year: i32, first: usize, last: usize) -> String {
        format!(
            "{EFSERVICE}/tri_facility/{}/tri_reporting_form/reporting_year/{year}/tri_release_qty/rows/{first}:{last}/JSON",
            area.filter()
        )
    }

    /// The unpaged query URL a reader can open to see the rows behind a summary.
    #[must_use]
    pub fn tri_source_url(area: &TriArea, year: i32) -> String {
        format!(
            "{EFSERVICE}/tri_facility/{}/tri_reporting_form/reporting_year/{year}/tri_release_qty/JSON",
            area.filter()
        )
    }

    /// Summarizes TRI facilities and total on-site releases (lbs) for an area and year.
    ///
    /// # Errors
    /// Returns an error when a page request fails or is not a JSON array.
    pub fn tri_summary(&self, area: &TriArea, year: i32) -> Result<TriSummary, Error> {
        let mut rows = Vec::new();
        let mut first = 0;
        loop {
            let page = self.get_json(&Self::tri_url(area, year, first, first + PAGE_ROWS - 1))?;
            let page = page
                .as_array()
                .ok_or_else(|| Error::Decode("TRI page was not an array".into()))?
                .clone();
            let len = page.len();
            rows.extend(page);
            if len < PAGE_ROWS {
                break;
            }
            first += PAGE_ROWS;
        }
        Ok(summarize_tri(&rows))
    }

    /// Lists NPL sites within `radius_km` of a point, nearest first.
    ///
    /// # Errors
    /// Returns an error when the request fails or the response lacks `features`.
    pub fn npl_sites_near(
        &self,
        lat: f64,
        lon: f64,
        radius_km: f64,
    ) -> Result<Vec<NplSite>, Error> {
        let value = self.get_json(&npl_url(lat, lon, radius_km))?;
        parse_npl(&value, lat, lon)
    }
}

fn backoff(url: &str, attempt: u32) -> Duration {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let digest = Sha256::digest(format!("{url}:{attempt}:{now}").as_bytes());
    let jitter = u64::from(u16::from_be_bytes([digest[0], digest[1]]) % 1000);
    Duration::from_millis(1000 * u64::from(attempt) + jitter)
}

/// Reads `TOTALQUERYRESULTS` from an Envirofacts `count` response.
#[must_use]
pub fn parse_count(value: &Value) -> u64 {
    value
        .pointer("/0/TOTALQUERYRESULTS")
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

/// Builds the `ArcGIS` point-radius query URL (attributes only, WGS84 degrees).
#[must_use]
pub fn npl_url(lat: f64, lon: f64, radius_km: f64) -> String {
    let meters = radius_km * 1000.0;
    format!(
        "{NPL_LAYER}/query?geometry={lon},{lat}&geometryType=esriGeometryPoint&inSR=4326&spatialRel=esriSpatialRelIntersects&distance={meters}&units=esriSRUnit_Meter&outFields=Site_Name,Site_EPA_ID,Status,City,State,Latitude,Longitude&returnGeometry=false&f=json"
    )
}

/// Parses an NPL query response into sites sorted nearest first.
///
/// # Errors
/// Returns an error when the response carries an `error` or no `features` array.
pub fn parse_npl(value: &Value, lat: f64, lon: f64) -> Result<Vec<NplSite>, Error> {
    if let Some(error) = value.get("error") {
        return Err(Error::Decode(format!("ArcGIS error: {error}")));
    }
    let features = value
        .get("features")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Decode("NPL response omitted features".into()))?;
    let text = |a: &Value, key: &str| {
        a.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let mut sites: Vec<NplSite> = features
        .iter()
        .filter_map(|feature| {
            let a = feature.get("attributes")?;
            let (site_lat, site_lon) =
                (a.get("Latitude")?.as_f64()?, a.get("Longitude")?.as_f64()?);
            Some(NplSite {
                name: text(a, "Site_Name"),
                epa_id: text(a, "Site_EPA_ID"),
                status: text(a, "Status"),
                city: text(a, "City"),
                state: text(a, "State"),
                lat: site_lat,
                lon: site_lon,
                distance_km: haversine_km(lat, lon, site_lat, site_lon),
            })
        })
        .collect();
    sites.sort_by(|a, b| a.distance_km.total_cmp(&b.distance_km));
    Ok(sites)
}

/// Totals `total_release` (pounds) across joined release rows and counts distinct facilities.
#[must_use]
pub fn summarize_tri(rows: &[Value]) -> TriSummary {
    let facilities: BTreeSet<&str> = rows
        .iter()
        .filter_map(|row| row.get("tri_facility_id").and_then(Value::as_str))
        .collect();
    let onsite_release_lbs = rows
        .iter()
        .filter_map(|row| row.get("total_release"))
        .filter_map(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
        .sum();
    TriSummary {
        facilities: facilities.len(),
        onsite_release_lbs,
    }
}

/// Great-circle distance in kilometres.
#[must_use]
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let rad = std::f64::consts::PI / 180.0;
    let (a, b) = (lat1 * rad, lat2 * rad);
    let h = ((b - a) / 2.0).sin().powi(2)
        + a.cos() * b.cos() * ((lon2 - lon1) * rad / 2.0).sin().powi(2);
    EARTH_RADIUS_KM * 2.0 * h.sqrt().atan2((1.0 - h).sqrt())
}
