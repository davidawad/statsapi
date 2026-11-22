//! Blocking client for U.S. IRS Statistics of Income public CSV datasets.
//!
//! The client downloads and decodes the ZIP income and county migration files.
//! Supply an optional cache directory with [`Client::with_cache_dir`].

use csv::StringRecord;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use thiserror::Error;

/// Current IRS SOI ZIP individual income dataset.
pub const ZIP_URL: &str = "https://www.irs.gov/pub/irs-soi/23zpallagi.csv";
/// Current IRS SOI county inflow dataset.
pub const COUNTY_IN_URL: &str = "https://www.irs.gov/pub/irs-soi/countyinflow2324.csv";
/// Current IRS SOI county outflow dataset.
pub const COUNTY_OUT_URL: &str = "https://www.irs.gov/pub/irs-soi/countyoutflow2324.csv";

/// Client errors from fetching, caching, or decoding IRS data.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// A network request or response failed.
    #[error("IRS request failed: {0}")]
    Request(String),
    /// Cache file access failed.
    #[error("IRS cache failed: {0}")]
    Cache(String),
    /// CSV data could not be decoded or did not have expected columns.
    #[error("IRS CSV is invalid: {0}")]
    Csv(String),
    /// No cached copy was available while offline mode was enabled.
    #[error("IRS data unavailable offline: {0}")]
    Offline(String),
}

/// Reusable blocking IRS SOI data client.
///
/// An in-memory copy of each dataset is kept for the lifetime of this client.
/// A supplied cache directory persists downloaded CSV files between clients.
#[derive(Debug)]
pub struct Client {
    cache_dir: Option<PathBuf>,
    offline: bool,
    zip_rows: Option<Vec<StringRecord>>,
    migration_rows: Option<(Vec<StringRecord>, Vec<StringRecord>)>,
}

impl Client {
    /// Creates a client without a persistent disk cache.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cache_dir: None,
            offline: false,
            zip_rows: None,
            migration_rows: None,
        }
    }

    /// Creates a client that stores downloads under the caller-supplied directory.
    #[must_use]
    pub fn with_cache_dir(path: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: Some(path.into()),
            offline: false,
            zip_rows: None,
            migration_rows: None,
        }
    }

    /// Prevents network access; requested files must already exist in the cache.
    #[must_use]
    pub fn offline(mut self) -> Self {
        self.offline = true;
        self
    }

    /// Returns a ZIP income metric for a five-digit ZIP code.
    ///
    /// # Errors
    ///
    /// Returns an error if the IRS download, optional disk cache, or CSV decoding fails.
    pub fn zip_metric(&mut self, zip: &str, metric: &str) -> Result<Option<f64>, Error> {
        if self.zip_rows.is_none() {
            self.zip_rows = Some(self.load_csv(ZIP_URL, "zip")?);
        }
        zip_value(self.zip_rows.as_deref().unwrap_or_default(), zip, metric)
    }

    /// Returns an inflow, outflow, or net migration metric for a county or state.
    ///
    /// # Errors
    ///
    /// Returns an error if either IRS download, optional disk cache, or CSV decoding fails.
    pub fn migration_metric(
        &mut self,
        level: &str,
        geo_id: &str,
        metric: &str,
    ) -> Result<Option<f64>, Error> {
        if self.migration_rows.is_none() {
            self.migration_rows = Some((
                self.load_csv(COUNTY_IN_URL, "migration-in")?,
                self.load_csv(COUNTY_OUT_URL, "migration-out")?,
            ));
        }
        let (inflow, outflow) = self
            .migration_rows
            .as_ref()
            .ok_or_else(|| Error::Csv("migration datasets were not loaded".to_owned()))?;
        let (incoming, outgoing) = migration_values(inflow, outflow, level, geo_id)?;
        let value = match metric {
            "in_migration_returns" => incoming.0,
            "out_migration_returns" => outgoing.0,
            "net_migration_returns" => incoming.0 - outgoing.0,
            "in_migration_agi" => incoming.1,
            "out_migration_agi" => outgoing.1,
            "net_migration_agi" => incoming.1 - outgoing.1,
            _ => return Ok(None),
        };
        Ok(Some(value))
    }

    fn load_csv(&self, url: &str, family: &str) -> Result<Vec<StringRecord>, Error> {
        let path = self
            .cache_dir
            .as_deref()
            .map(|dir| cache_path(dir, url, family));
        let bytes = if let Some(path) = path.as_ref().filter(|path| path.exists()) {
            fs::read(path)
                .map_err(|error| Error::Cache(format!("reading {}: {error}", path.display())))?
        } else {
            if self.offline {
                return Err(Error::Offline(format!("cache miss for {url}")));
            }
            let bytes = fetch_with_retry(url)?;
            if let Some(path) = path {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        Error::Cache(format!("creating {}: {error}", parent.display()))
                    })?;
                }
                let temp = path.with_extension(format!("tmp-{}", std::process::id()));
                fs::write(&temp, &bytes).map_err(|error| {
                    Error::Cache(format!("writing {}: {error}", temp.display()))
                })?;
                fs::rename(&temp, &path).map_err(|error| {
                    Error::Cache(format!("installing {}: {error}", path.display()))
                })?;
            }
            bytes
        };
        parse_rows(&bytes, url)
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

fn fetch_with_retry(url: &str) -> Result<Vec<u8>, Error> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("statsapi-irs-soi/0.1 (public data client)")
        .build()
        .map_err(|error| Error::Request(error.to_string()))?;
    let mut last_error = String::new();
    for attempt in 0..4 {
        let result = client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .and_then(reqwest::blocking::Response::bytes);
        match result {
            Ok(bytes) => return Ok(bytes.to_vec()),
            Err(error) => {
                let retryable = error.is_timeout()
                    || error.is_connect()
                    || error.is_body()
                    || error
                        .status()
                        .is_some_and(|status| status.as_u16() == 429 || status.is_server_error());
                last_error = error.to_string();
                if !retryable || attempt == 3 {
                    break;
                }
                let base_ms = 200_u64.saturating_mul(1_u64 << attempt);
                thread::sleep(Duration::from_millis(
                    base_ms + u64::from(fastrand::u8(..40)),
                ));
            }
        }
    }
    Err(Error::Request(format!("fetching {url}: {last_error}")))
}

fn cache_path(root: &Path, url: &str, family: &str) -> PathBuf {
    let key = format!("{}-{}", family, hex(&Sha256::digest(url.as_bytes())));
    root.join(&key[..2]).join(format!("{key}.csv"))
}

fn parse_rows(bytes: &[u8], url: &str) -> Result<Vec<StringRecord>, Error> {
    let mut reader = csv::ReaderBuilder::new().flexible(true).from_reader(bytes);
    let mut rows = vec![reader
        .headers()
        .map_err(|error| Error::Csv(format!("reading header {url}: {error}")))?
        .clone()];
    rows.extend(
        reader
            .records()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| Error::Csv(format!("parsing {url}: {error}")))?,
    );
    Ok(rows)
}

/// Parses and computes a ZIP metric from a CSV document, useful for offline workflows.
///
/// # Errors
///
/// Returns an error when the CSV is malformed, required columns are absent, or a value is invalid.
pub fn parse_zip_metric(csv: &str, zip: &str, metric: &str) -> Result<Option<f64>, Error> {
    zip_value(&parse_rows(csv.as_bytes(), "fixture")?, zip, metric)
}

/// Parses and computes a migration metric from two CSV documents, useful offline.
///
/// # Errors
///
/// Returns an error when either CSV is malformed, required columns are absent, or a value is invalid.
pub fn parse_migration_metric(
    inflow_csv: &str,
    outflow_csv: &str,
    level: &str,
    id: &str,
    metric: &str,
) -> Result<Option<f64>, Error> {
    let incoming = parse_rows(inflow_csv.as_bytes(), "inflow fixture")?;
    let outgoing = parse_rows(outflow_csv.as_bytes(), "outflow fixture")?;
    let (incoming, outgoing) = migration_values(&incoming, &outgoing, level, id)?;
    Ok(match metric {
        "in_migration_returns" => Some(incoming.0),
        "out_migration_returns" => Some(outgoing.0),
        "net_migration_returns" => Some(incoming.0 - outgoing.0),
        "in_migration_agi" => Some(incoming.1),
        "out_migration_agi" => Some(outgoing.1),
        "net_migration_agi" => Some(incoming.1 - outgoing.1),
        _ => None,
    })
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn number(record: &StringRecord, headers: &StringRecord, name: &str) -> Result<f64, Error> {
    let index = headers
        .iter()
        .position(|header| header.eq_ignore_ascii_case(name))
        .ok_or_else(|| Error::Csv(format!("missing {name} column")))?;
    let raw = record.get(index).unwrap_or("").trim();
    if raw.is_empty() || raw == "-" {
        return Ok(0.0);
    }
    raw.parse::<f64>()
        .map_err(|error| Error::Csv(format!("invalid {name} value {raw:?}: {error}")))
}

fn zip_value(rows: &[StringRecord], zip: &str, metric: &str) -> Result<Option<f64>, Error> {
    let Some(headers) = rows.first() else {
        return Ok(None);
    };
    let mut totals = BTreeMap::<String, f64>::new();
    let mut high_returns = 0.0;
    for row in rows
        .iter()
        .filter(|row| row.get(2).is_some_and(|value| value == zip))
    {
        for column in [
            "N1", "A00100", "A00200", "A00600", "A01000", "N59660", "N04470",
        ] {
            *totals.entry(column.to_owned()).or_default() += number(row, headers, column)?;
        }
        if row.get(3) == Some("6") {
            high_returns += number(row, headers, "N1")?;
        }
    }
    let get = |name: &str| totals.get(name).copied().unwrap_or_default();
    let returns = get("N1");
    if returns == 0.0 {
        return Ok(None);
    }
    Ok(match metric {
        "returns" => Some(returns),
        "agi" => Some(get("A00100")),
        "mean_agi" => Some(get("A00100") / returns),
        "wages" => Some(get("A00200")),
        "dividends" => Some(get("A00600")),
        "capital_gains" => Some(get("A01000")),
        "eitc_share" => Some(get("N59660") / returns),
        "agi_200k_share" => Some(high_returns / returns),
        "itemizers_share" => Some(get("N04470") / returns),
        _ => None,
    })
}

type CountAndAgi = (f64, f64);
type MigrationFlows = (CountAndAgi, CountAndAgi);

fn migration_values(
    inflow: &[StringRecord],
    outflow: &[StringRecord],
    level: &str,
    id: &str,
) -> Result<MigrationFlows, Error> {
    let parse = |rows: &[StringRecord], incoming: bool| -> Result<(f64, f64), Error> {
        let Some(headers) = rows.first() else {
            return Ok((0.0, 0.0));
        };
        let (state_col, county_col, label_col) = if incoming {
            ("y2_statefips", "y2_countyfips", "y1_countyname")
        } else {
            ("y1_statefips", "y1_countyfips", "y2_countyname")
        };
        let mut returns = 0.0;
        let mut agi = 0.0;
        for row in rows {
            let column = headers
                .iter()
                .position(|field| field == label_col)
                .unwrap_or(0);
            if !row
                .get(column)
                .unwrap_or("")
                .ends_with("Total Migration-US and Foreign")
            {
                continue;
            }
            let state = headers
                .iter()
                .position(|field| field == state_col)
                .and_then(|index| row.get(index))
                .unwrap_or("");
            let county = headers
                .iter()
                .position(|field| field == county_col)
                .and_then(|index| row.get(index))
                .unwrap_or("");
            let matches = match level {
                "county" => format!("{state}{county}") == id,
                "state" => state == id,
                _ => false,
            };
            if matches {
                returns += number(row, headers, "n1")?;
                agi += number(row, headers, "agi")?;
            }
        }
        Ok((returns, agi))
    };
    Ok((parse(inflow, true)?, parse(outflow, false)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_fixtures_are_synthetic_and_metrics_decode() {
        let zip = include_str!("../tests/fixtures/zip.csv");
        assert_eq!(
            parse_zip_metric(zip, "12345", "returns").unwrap(),
            Some(15.0)
        );
        assert_eq!(
            parse_zip_metric(zip, "12345", "mean_agi").unwrap(),
            Some(200.0)
        );
        assert_eq!(
            parse_zip_metric(zip, "12345", "agi_200k_share").unwrap(),
            Some(1.0 / 3.0)
        );
    }

    #[test]
    fn migration_fixture_decodes_in_out_and_net() {
        let inflow = include_str!("../tests/fixtures/county-in.csv");
        let outflow = include_str!("../tests/fixtures/county-out.csv");
        assert_eq!(
            parse_migration_metric(inflow, outflow, "county", "99001", "net_migration_returns")
                .unwrap(),
            Some(4.0)
        );
        assert_eq!(
            parse_migration_metric(inflow, outflow, "county", "99001", "net_migration_agi")
                .unwrap(),
            Some(450.0)
        );
    }

    #[test]
    fn offline_client_reports_cache_misses_without_network_access() {
        let cache = std::env::temp_dir().join(format!(
            "statsapi-irs-soi-offline-test-{}",
            std::process::id()
        ));
        let result = Client::with_cache_dir(cache)
            .offline()
            .zip_metric("12345", "returns");
        assert!(matches!(result, Err(Error::Offline(_))));
    }
}
