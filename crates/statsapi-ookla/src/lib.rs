//! Parser and blocking helpers for Ookla Open Data performance tiles.

use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::{Field, Row};
use std::io::Read;
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

/// Ookla performance tile root.
pub const BASE_URL: &str = "https://ookla-open-data.s3.amazonaws.com/parquet/performance";
/// The tile zoom used by the open performance release.
pub const ZOOM: u8 = 12;

/// Weighted totals for a group of Ookla tiles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Totals {
    /// Number of tests.
    pub tests: f64,
    /// Number of devices.
    pub devices: f64,
    /// Test-weighted download total, in kbps.
    pub download_test_sum: f64,
    /// Test-weighted upload total, in kbps.
    pub upload_test_sum: f64,
    /// Test-weighted latency total, in milliseconds.
    pub latency_test_sum: f64,
}

/// Errors returned while downloading or decoding a tile file.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// Network or HTTP failure.
    #[error("Ookla request failed: {0}")]
    Request(String),
    /// Parquet or row decoding failure.
    #[error("Ookla parquet is invalid: {0}")]
    Decode(String),
}

/// One decoded performance tile row.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    /// Quadkey at the release's zoom.
    pub quadkey: String,
    pub avg_download_kbps: f64,
    pub avg_upload_kbps: f64,
    pub avg_latency_ms: f64,
    pub tests: f64,
    pub devices: f64,
}

impl Totals {
    /// Adds a row using test-weighted speed and latency.
    pub fn include(&mut self, tile: &Tile) {
        let tests = tile.tests.max(0.0);
        self.tests += tests;
        self.devices += tile.devices.max(0.0);
        self.download_test_sum += tile.avg_download_kbps * tests;
        self.upload_test_sum += tile.avg_upload_kbps * tests;
        self.latency_test_sum += tile.avg_latency_ms * tests;
    }
}

/// Extracts matching rows from a parquet file and deletes nothing.
///
/// # Errors
///
/// Returns an error when the file cannot be opened or decoded as parquet.
pub fn aggregate_file(path: &Path, prefixes: &[String]) -> Result<Totals, Error> {
    let reader = SerializedFileReader::new(
        std::fs::File::open(path).map_err(|e| Error::Decode(e.to_string()))?,
    )
    .map_err(|e| Error::Decode(e.to_string()))?;
    aggregate_rows(
        reader
            .get_row_iter(None)
            .map_err(|e| Error::Decode(e.to_string()))?,
        prefixes,
    )
}

/// Streams a parquet body to a temporary caller-owned path before parsing it.
///
/// # Errors
///
/// Returns an error when the body cannot be written or the resulting file is invalid parquet.
pub fn aggregate_reader(
    mut body: impl Read,
    prefixes: &[String],
    path: &Path,
) -> Result<Totals, Error> {
    let mut file = std::fs::File::create(path).map_err(|e| Error::Decode(e.to_string()))?;
    std::io::copy(&mut body, &mut file).map_err(|e| Error::Decode(e.to_string()))?;
    drop(file);
    aggregate_file(path, prefixes)
}

fn aggregate_rows<I>(rows: I, prefixes: &[String]) -> Result<Totals, Error>
where
    I: Iterator<Item = Result<Row, parquet::errors::ParquetError>>,
{
    let mut totals = Totals::default();
    for row in rows {
        let row = row.map_err(|e| Error::Decode(e.to_string()))?;
        let quadkey = text(&row, "quadkey");
        if prefixes.iter().any(|prefix| quadkey.starts_with(prefix)) {
            totals.include(&Tile {
                quadkey,
                avg_download_kbps: number(&row, "avg_d_kbps"),
                avg_upload_kbps: number(&row, "avg_u_kbps"),
                avg_latency_ms: number(&row, "avg_lat_ms"),
                tests: number(&row, "tests"),
                devices: number(&row, "devices"),
            });
        }
    }
    Ok(totals)
}

fn text(row: &Row, name: &str) -> String {
    row.get_column_iter()
        .find(|(column, _)| *column == name)
        .map_or_else(String::new, |(_, field)| match field {
            Field::Null => String::new(),
            Field::Str(value) => value.clone(),
            other => other.to_string().trim_matches('"').to_owned(),
        })
}

fn number(row: &Row, name: &str) -> f64 {
    text(row, name).parse().unwrap_or(0.0)
}

/// Returns the nine quadkey prefixes around a point.
#[must_use]
pub fn prefixes(lat: f64, lon: f64) -> Vec<String> {
    let (x, y) = tile_xy(lat, lon, ZOOM);
    let n = 1_i64 << ZOOM;
    (-1..=1)
        .flat_map(|dy| {
            (-1..=1).map(move |dx| quadkey((x + dx).rem_euclid(n), (y + dy).clamp(0, n - 1), ZOOM))
        })
        .collect()
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "Web Mercator indices are bounded by zoom."
)]
fn tile_xy(lat: f64, lon: f64, zoom: u8) -> (i64, i64) {
    let n = 1_i64 << zoom;
    let x = (((lon + 180.0) / 360.0) * n as f64).floor() as i64;
    let r = lat.to_radians();
    let y = ((1.0 - (r.tan() + 1.0 / r.cos()).ln() / std::f64::consts::PI) / 2.0 * n as f64).floor()
        as i64;
    (x.clamp(0, n - 1), y.clamp(0, n - 1))
}

fn quadkey(x: i64, y: i64, zoom: u8) -> String {
    (1..=zoom)
        .rev()
        .map(|level| {
            let mask = 1_i64 << (level - 1);
            let digit = i64::from(x & mask != 0) + 2 * i64::from(y & mask != 0);
            char::from(b'0' + u8::try_from(digit).unwrap_or(0))
        })
        .collect()
}

/// Converts weighted totals to the adapter's supported metrics.
#[must_use]
pub fn value(totals: &Totals, metric: &str) -> Option<f64> {
    match metric {
        "download_mbps" if totals.tests > 0.0 => {
            Some(totals.download_test_sum / totals.tests / 1000.0)
        }
        "upload_mbps" if totals.tests > 0.0 => Some(totals.upload_test_sum / totals.tests / 1000.0),
        "latency_ms" if totals.tests > 0.0 => Some(totals.latency_test_sum / totals.tests),
        "tests" => Some(totals.tests),
        "devices" => Some(totals.devices),
        _ => None,
    }
}

/// Downloads one release object without caching it.
///
/// # Errors
///
/// Returns an error when the HTTP client cannot be initialized or the request fails.
pub fn download(url: &str) -> Result<reqwest::blocking::Response, Error> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_mins(15))
        .user_agent("statsapi-ookla/0.1 (+https://github.com/davidawad/statsapi)")
        .build()
        .map_err(|e| Error::Request(e.to_string()))?
        .get(url)
        .send()
        .map_err(|e| Error::Request(e.to_string()))?
        .error_for_status()
        .map_err(|e| Error::Request(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn values_use_test_weighting() {
        let totals = Totals {
            tests: 4.0,
            devices: 2.0,
            download_test_sum: 120_000.0,
            upload_test_sum: 20_000.0,
            latency_test_sum: 80.0,
        };
        assert_eq!(value(&totals, "download_mbps"), Some(30.0));
        assert_eq!(value(&totals, "latency_ms"), Some(20.0));
    }
}
