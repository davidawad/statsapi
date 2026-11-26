//! Blocking client for the Eurostat dissemination API and GISCO NUTS boundaries.
//!
//! Nothing is written to disk: every call is a live request, and boundary
//! files are streamed into memory, searched, and dropped.

mod error;
mod nuts;
mod stat;

pub use error::Error;
pub use nuts::{NutsLevel, NutsRegion};
pub use stat::Observation;

use serde_json::Value;
use std::io::Read;
use std::time::Duration;

const DATA_BASE: &str = "https://ec.europa.eu/eurostat/api/dissemination/statistics/1.0/data";
const GISCO_BASE: &str = "https://gisco-services.ec.europa.eu/distribution/v2/nuts/geojson";
const MAX_ATTEMPTS: u32 = 4;
/// Upper bound on one response body; the largest GISCO file used is about 2.6 MB.
const MAX_BODY_BYTES: u64 = 16 * 1024 * 1024;
/// How many trailing periods are requested so a null latest year can be skipped.
const RECENT_PERIODS: u32 = 6;

/// A blocking Eurostat and GISCO client.
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
            .user_agent(concat!("statsapi-eurostat/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(Error::Http)?;
        Ok(Self { http })
    }

    /// Builds the dissemination URL for a dataset restricted to `filters`.
    ///
    /// Each `(dimension, code)` pair pins one dimension; `geo` is passed like
    /// any other. Only the trailing periods are requested.
    #[must_use]
    pub fn data_url(dataset: &str, filters: &[(&str, &str)]) -> String {
        let mut url = format!("{DATA_BASE}/{dataset}?format=JSON&lang=en");
        for (dimension, code) in filters {
            url.push('&');
            url.push_str(dimension);
            url.push('=');
            url.push_str(code);
        }
        url.push_str("&lastTimePeriod=");
        url.push_str(&RECENT_PERIODS.to_string());
        url
    }

    /// Gets the newest non-null observation of a fully pinned series.
    ///
    /// Returns `Ok(None)` when the region has no published value.
    ///
    /// # Errors
    /// Returns an error for transport failures, invalid JSON, or a filter set
    /// that leaves more than one series.
    pub fn latest(
        &self,
        dataset: &str,
        filters: &[(&str, &str)],
    ) -> Result<Option<Observation>, Error> {
        let url = Self::data_url(dataset, filters);
        let body = self.get(&url)?;
        let payload: Value = serde_json::from_slice(&body).map_err(Error::Json)?;
        stat::latest_observation(&payload)
    }

    /// Finds the NUTS region of `level` containing a WGS84 point.
    ///
    /// The GISCO boundary file is streamed into memory and discarded.
    ///
    /// # Errors
    /// Returns an error for transport failures or an unreadable boundary file.
    pub fn nuts_for_point(
        &self,
        lat: f64,
        lon: f64,
        level: NutsLevel,
        year: u16,
    ) -> Result<Option<NutsRegion>, Error> {
        let url = Self::boundary_url(level, year);
        let body = self.get(&url)?;
        let payload: Value = serde_json::from_slice(&body).map_err(Error::Json)?;
        nuts::region_containing(&payload, lat, lon, level)
    }

    /// Builds the GISCO boundary URL (1:10 million generalisation) for a level and NUTS year.
    #[must_use]
    pub fn boundary_url(level: NutsLevel, year: u16) -> String {
        format!(
            "{GISCO_BASE}/NUTS_RG_10M_{year}_4326_LEVL_{}.geojson",
            level.digit()
        )
    }

    fn get(&self, url: &str) -> Result<Vec<u8>, Error> {
        for attempt in 0..MAX_ATTEMPTS {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    let mut body = Vec::new();
                    response
                        .take(MAX_BODY_BYTES)
                        .read_to_end(&mut body)
                        .map_err(Error::Io)?;
                    return Ok(body);
                }
                Ok(response) => {
                    let status = response.status();
                    let retryable = status.as_u16() == 429 || status.is_server_error();
                    if !retryable || attempt + 1 == MAX_ATTEMPTS {
                        return Err(Error::HttpStatus(status.as_u16()));
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
