//! Blocking client for FCC Broadband Data Collection availability files.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{Cursor, Read};
use std::time::Duration;
use thiserror::Error;

/// FCC BDC public map API root.
pub const API_BASE: &str = "https://bdc.fcc.gov/api/public/map";
const COUNTY_SUMMARY: &str = "Summary by Geography Type - Other Geographies";
const PROVIDER_SUMMARY: &str = "Provider Summary by Geography Type";

/// Caller-supplied FCC BDC request headers.
#[derive(Clone, Debug)]
pub struct Credentials {
    username: String,
    token: String,
}

impl Credentials {
    /// Creates headers after rejecting empty values.
    pub fn new(username: impl Into<String>, token: impl Into<String>) -> Option<Self> {
        let username = username.into().trim().to_owned();
        let token = token.into().trim().to_owned();
        (!username.is_empty() && !token.is_empty()).then_some(Self { username, token })
    }
}

/// County figures from one BDC release.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CountyAvailability {
    /// Release date returned by the BDC listing.
    pub as_of: String,
    /// Total residential and business units in the county file.
    pub total_units: f64,
    /// Share served at 100/20 Mbps, when reported.
    pub share_100_20: Option<f64>,
    /// Number of providers reporting residential coverage.
    pub provider_count: usize,
}

/// Errors returned by the BDC client.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// Network or HTTP failure.
    #[error("FCC BDC request failed: {0}")]
    Request(String),
    /// Invalid JSON, ZIP, or CSV data.
    #[error("FCC BDC response is invalid: {0}")]
    Decode(String),
    /// The release does not contain the required file.
    #[error("FCC BDC release is incomplete: {0}")]
    Missing(String),
}

/// A blocking BDC client. Credentials and storage policy belong to the caller.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    base: String,
    credentials: Credentials,
}

impl Client {
    /// Creates a client against the production endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be initialized.
    pub fn new(credentials: Credentials) -> Result<Self, Error> {
        Self::with_base(API_BASE, credentials)
    }

    /// Creates a client against an alternate endpoint for tests.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be initialized.
    pub fn with_base(base: &str, credentials: Credentials) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(300))
            .user_agent("statsapi-fcc-bdc/0.1 (+https://github.com/davidawad/statsapi)")
            .build()
            .map_err(|e| Error::Request(e.to_string()))?;
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
            credentials,
        })
    }

    fn get(&self, url: &str) -> Result<Vec<u8>, Error> {
        self.http
            .get(url)
            .header("username", &self.credentials.username)
            .header("hash_value", &self.credentials.token)
            .send()
            .map_err(|e| Error::Request(e.to_string()))?
            .error_for_status()
            .map_err(|e| Error::Request(e.to_string()))?
            .bytes()
            .map(|b| b.to_vec())
            .map_err(|e| Error::Request(e.to_string()))
    }

    /// Downloads and parses the latest county availability release.
    ///
    /// # Errors
    ///
    /// Returns an error when the release listing, ZIP, or CSV is unavailable or invalid.
    pub fn county(&self, county: &str) -> Result<CountyAvailability, Error> {
        let json = |path: &str| -> Result<Value, Error> {
            serde_json::from_slice(&self.get(&format!("{}/{}", self.base, path))?)
                .map_err(|e| Error::Decode(e.to_string()))
        };
        let dates = json("listAsOfDates")?;
        let as_of = dates
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|row| row.get("data_type").and_then(Value::as_str) == Some("availability"))
            .filter_map(|row| row.get("as_of_date").and_then(Value::as_str))
            .max()
            .map(str::to_owned)
            .ok_or_else(|| Error::Missing("no availability release listed".into()))?;
        let listing = json(&format!("downloads/listAvailabilityData/{as_of}"))?;
        let file_id = |subcategory: &str, technology: Option<&str>| {
            listing
                .get("data")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find_map(|row| {
                    let ok = row.get("category").and_then(Value::as_str) == Some("Summary")
                        && row.get("subcategory").and_then(Value::as_str) == Some(subcategory)
                        && technology.is_none_or(|t| {
                            row.get("technology_type").and_then(Value::as_str) == Some(t)
                        });
                    ok.then(|| row.get("file_id").and_then(Value::as_u64))
                        .flatten()
                })
        };
        let summary = file_id(COUNTY_SUMMARY, Some("Fixed Broadband"))
            .ok_or_else(|| Error::Missing("county summary file not listed".into()))?;
        let providers = file_id(PROVIDER_SUMMARY, None)
            .ok_or_else(|| Error::Missing("provider summary file not listed".into()))?;
        let download = |id| {
            self.get(&format!(
                "{}/downloads/downloadFile/availability/{id}/1",
                self.base
            ))
        };
        let (share_100_20, total_units) = county_share(download(summary)?, county)?;
        Ok(CountyAvailability {
            as_of,
            total_units,
            share_100_20,
            provider_count: county_providers(download(providers)?, county)?,
        })
    }
}

fn reader(bytes: Vec<u8>) -> Result<csv::Reader<Cursor<Vec<u8>>>, Error> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| Error::Decode(e.to_string()))?;
    let mut entry = archive
        .by_index(0)
        .map_err(|e| Error::Decode(e.to_string()))?;
    let mut text = Vec::new();
    entry
        .read_to_end(&mut text)
        .map_err(|e| Error::Decode(e.to_string()))?;
    Ok(csv::Reader::from_reader(Cursor::new(text)))
}

fn column(headers: &csv::StringRecord, name: &str) -> Result<usize, Error> {
    headers
        .iter()
        .position(|header| header == name)
        .ok_or_else(|| Error::Decode(format!("BDC file lacks column {name}")))
}

fn county_share(bytes: Vec<u8>, county: &str) -> Result<(Option<f64>, f64), Error> {
    let mut csv = reader(bytes)?;
    let headers = csv
        .headers()
        .map_err(|e| Error::Decode(e.to_string()))?
        .clone();
    let [kind, geo, id, units, residential, technology, speed] = [
        "area_data_type",
        "geography_type",
        "geography_id",
        "total_units",
        "biz_res",
        "technology",
        "speed_100_20",
    ]
    .map(|name| column(&headers, name));
    let kind = kind?;
    let geo = geo?;
    let id = id?;
    let units = units?;
    let residential = residential?;
    let technology = technology?;
    let speed = speed?;
    for row in csv.records() {
        let row = row.map_err(|e| Error::Decode(e.to_string()))?;
        if row.get(kind) == Some("Total")
            && row.get(geo) == Some("County")
            && row.get(id) == Some(county)
            && row.get(residential) == Some("R")
            && row.get(technology) == Some("Any Technology")
        {
            let total = row.get(units).and_then(|v| v.parse().ok()).unwrap_or(0.0);
            return Ok((row.get(speed).and_then(|v| v.parse().ok()), total));
        }
    }
    Ok((None, 0.0))
}

fn county_providers(bytes: Vec<u8>, county: &str) -> Result<usize, Error> {
    let mut csv = reader(bytes)?;
    let headers = csv
        .headers()
        .map_err(|e| Error::Decode(e.to_string()))?
        .clone();
    let geo = column(&headers, "geography_type")?;
    let id = column(&headers, "geography_id")?;
    let kind = column(&headers, "data_type")?;
    let provider = column(&headers, "provider_id")?;
    let residential = column(&headers, "res_st_pct")?;
    let mut providers = std::collections::BTreeSet::new();
    for row in csv.records() {
        let row = row.map_err(|e| Error::Decode(e.to_string()))?;
        let covered = row
            .get(residential)
            .and_then(|v| v.parse::<f64>().ok())
            .is_some_and(|v| v > 0.0);
        if row.get(geo) == Some("County")
            && row.get(id) == Some(county)
            && row.get(kind) == Some("Fixed Broadband")
            && covered
        {
            providers.insert(row.get(provider).unwrap_or_default().to_owned());
        }
    }
    Ok(providers.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_reject_empty_values() {
        assert!(Credentials::new("", "token").is_none());
        assert!(Credentials::new("user", " token ").is_some());
    }
}
