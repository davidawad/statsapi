//! Blocking client for the FHFA annual house price index (HPI) files.
//!
//! FHFA publishes the ZIP, three-digit ZIP, county and tract indexes as bulk
//! files, with no query API. This client streams the file for one geography
//! level, keeps only the rows of the requested geography, and persists
//! nothing: no cache, no temp file.

use calamine::{Data, Reader, Xlsx};
use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::thread;
use std::time::Duration;
use thiserror::Error;

/// Base URL of FHFA's annual HPI downloads.
pub const BASE_URL: &str = "https://www.fhfa.gov/hpi/download/annual";
/// Largest download the client buffers in memory, in bytes (the ZIP5 workbook is about 40 MB).
const MAX_WORKBOOK_BYTES: u64 = 160 * 1024 * 1024;
/// Per-request timeout; the tract CSV is about 90 MB.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(600);
/// Attempts per download, including the first.
const ATTEMPTS: u32 = 4;

/// Geography level of an FHFA annual HPI file.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Level {
    /// Five-digit ZIP code (`hpi_at_zip5.xlsx`).
    Zip5,
    /// Three-digit ZIP code prefix (`hpi_at_zip3.xlsx`).
    Zip3,
    /// County, keyed by five-digit FIPS (`hpi_at_county.xlsx`).
    County,
    /// Census tract, keyed by eleven-digit FIPS (`hpi_at_tract.csv`).
    Tract,
}

impl Level {
    /// Every level, coarsest file first.
    pub const ALL: [Self; 4] = [Self::Zip3, Self::Zip5, Self::County, Self::Tract];

    /// URL of the bulk file for this level.
    #[must_use]
    pub fn url(self) -> String {
        let file = match self {
            Self::Zip5 => "hpi_at_zip5.xlsx",
            Self::Zip3 => "hpi_at_zip3.xlsx",
            Self::County => "hpi_at_county.xlsx",
            Self::Tract => "hpi_at_tract.csv",
        };
        format!("{BASE_URL}/{file}")
    }

    /// Column of the geography key in the XLSX layout (the county file prefixes state and name).
    const fn xlsx_key_column(self) -> usize {
        match self {
            Self::County => 2,
            _ => 0,
        }
    }
}

/// Errors from streaming or decoding an FHFA file.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// A network request or response failed.
    #[error("FHFA request failed: {0}")]
    Request(String),
    /// The workbook or CSV could not be decoded or lacked the expected columns.
    #[error("FHFA file is invalid: {0}")]
    Format(String),
}

/// One year of an index series.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    /// Index value (first year of the series is 100).
    pub hpi: f64,
    /// FHFA's published annual change, percent; absent for the first year or a missing value.
    pub annual_change: Option<f64>,
}

/// The annual index series of one geography.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Series {
    points: BTreeMap<u16, Point>,
}

/// Newest-year summary of a [`Series`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Latest {
    /// Newest year with a published index.
    pub year: u16,
    /// Index value for that year.
    pub hpi: f64,
    /// One-year change in percent.
    pub change_1y: Option<f64>,
    /// Five-year cumulative change in percent.
    pub change_5y: Option<f64>,
    /// Ten-year cumulative change in percent.
    pub change_10y: Option<f64>,
}

impl Series {
    /// Whether the geography has no published index.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Summarizes the newest year; `None` when the series is empty.
    #[must_use]
    pub fn latest(&self) -> Option<Latest> {
        let (&year, &point) = self.points.iter().next_back()?;
        let change_over = |years: u16| {
            let earlier = self.points.get(&year.checked_sub(years)?)?;
            (earlier.hpi != 0.0).then(|| (point.hpi / earlier.hpi - 1.0) * 100.0)
        };
        Some(Latest {
            year,
            hpi: point.hpi,
            change_1y: point.annual_change.or_else(|| change_over(1)),
            change_5y: change_over(5),
            change_10y: change_over(10),
        })
    }

    fn insert(&mut self, year: u16, point: Point) {
        self.points.insert(year, point);
    }
}

/// Streaming FHFA HPI client.
#[derive(Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
}

impl Client {
    /// Creates a client.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn new() -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .user_agent("statsapi-fhfa/0.1 (public data client)")
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| Error::Request(error.to_string()))?;
        Ok(Self { http })
    }

    /// Streams the file for `level` and returns the series of the geography `key`.
    ///
    /// `key` is the ZIP, three-digit prefix, county FIPS or tract FIPS as published
    /// (leading zeros kept). Nothing is written to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the download fails after retries or the file cannot be decoded.
    pub fn series(&self, level: Level, key: &str) -> Result<Series, Error> {
        let url = level.url();
        let response = self.get_with_retry(&url)?;
        if level == Level::Tract {
            return parse_tract_csv(response, key);
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_WORKBOOK_BYTES)
            .read_to_end(&mut bytes)
            .map_err(|error| Error::Request(format!("reading {url}: {error}")))?;
        parse_xlsx(level, &bytes, key)
    }

    fn get_with_retry(&self, url: &str) -> Result<reqwest::blocking::Response, Error> {
        let mut last_error = String::new();
        for attempt in 0..ATTEMPTS {
            match self
                .http
                .get(url)
                .send()
                .and_then(reqwest::blocking::Response::error_for_status)
            {
                Ok(response) => return Ok(response),
                Err(error) => {
                    let retryable = error.is_timeout()
                        || error.is_connect()
                        || error.status().is_some_and(|status| {
                            status.as_u16() == 429 || status.is_server_error()
                        });
                    last_error = error.to_string();
                    if !retryable || attempt + 1 == ATTEMPTS {
                        break;
                    }
                    let base_ms = 300_u64.saturating_mul(1_u64 << attempt);
                    thread::sleep(Duration::from_millis(
                        base_ms + u64::from(fastrand::u8(..60)),
                    ));
                }
            }
        }
        Err(Error::Request(format!("fetching {url}: {last_error}")))
    }
}

/// Decodes the series of `key` from FHFA XLSX bytes (ZIP5, ZIP3 or county layout).
///
/// # Errors
///
/// Returns an error when the workbook is unreadable or has no `Year`/`HPI` header row.
pub fn parse_xlsx(level: Level, bytes: &[u8], key: &str) -> Result<Series, Error> {
    let mut workbook: Xlsx<_> = calamine::open_workbook_from_rs(Cursor::new(bytes))
        .map_err(|error| Error::Format(format!("opening workbook: {error}")))?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or_else(|| Error::Format("workbook has no sheets".to_owned()))?
        .map_err(|error| Error::Format(format!("reading sheet: {error}")))?;
    let key_column = level.xlsx_key_column();
    let mut columns: Option<(usize, usize, usize)> = None;
    let mut series = Series::default();
    for row in range.rows() {
        let Some((year_col, change_col, hpi_col)) = columns else {
            columns = header_columns(row);
            continue;
        };
        if row.get(key_column).map(cell_text).as_deref() != Some(key) {
            continue;
        }
        let year = row.get(year_col).and_then(cell_number);
        let hpi = row.get(hpi_col).and_then(cell_number);
        if let (Some(year), Some(hpi)) = (year, hpi) {
            series.insert(
                year_of(year)?,
                Point {
                    hpi,
                    annual_change: row.get(change_col).and_then(cell_number),
                },
            );
        }
    }
    if columns.is_none() {
        return Err(Error::Format("no Year/HPI header row found".to_owned()));
    }
    Ok(series)
}

/// Locates the Year, Annual Change and HPI columns in a worksheet row.
fn header_columns(row: &[Data]) -> Option<(usize, usize, usize)> {
    let find = |name: &str| {
        row.iter()
            .position(|cell| cell_text(cell).trim().eq_ignore_ascii_case(name))
    };
    Some((find("Year")?, find("Annual Change (%)")?, find("HPI")?))
}

fn cell_text(cell: &Data) -> String {
    match cell {
        Data::String(text) => text.clone(),
        Data::Float(number) if number.fract() == 0.0 => format!("{number:.0}"),
        Data::Int(number) => number.to_string(),
        _ => String::new(),
    }
}

/// FHFA marks a missing value with a period.
fn cell_number(cell: &Data) -> Option<f64> {
    match cell {
        Data::Float(number) => Some(*number),
        #[expect(clippy::cast_precision_loss, reason = "index and year cells are small")]
        Data::Int(number) => Some(*number as f64),
        Data::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn year_of(value: f64) -> Result<u16, Error> {
    if (1900.0..=2200.0).contains(&value) {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "range checked"
        )]
        Ok(value as u16)
    } else {
        Err(Error::Format(format!("implausible year {value}")))
    }
}

/// Decodes the series of tract `key` from the streamed tract CSV.
///
/// Rows are grouped by tract, so reading stops after the tract's block ends.
///
/// # Errors
///
/// Returns an error when the CSV is unreadable or lacks `tract`, `year` or `hpi` columns.
pub fn parse_tract_csv(reader: impl Read, key: &str) -> Result<Series, Error> {
    let mut csv_reader = csv::Reader::from_reader(reader);
    let headers = csv_reader
        .headers()
        .map_err(|error| Error::Format(format!("reading tract header: {error}")))?
        .clone();
    let column = |name: &str| {
        headers
            .iter()
            .position(|header| header == name)
            .ok_or_else(|| Error::Format(format!("tract file lacks {name} column")))
    };
    let (tract_col, year_col, change_col, hpi_col) = (
        column("tract")?,
        column("year")?,
        column("annual_change")?,
        column("hpi")?,
    );
    let mut series = Series::default();
    for record in csv_reader.records() {
        let record =
            record.map_err(|error| Error::Format(format!("reading tract row: {error}")))?;
        if record.get(tract_col) != Some(key) {
            if series.is_empty() {
                continue;
            }
            break;
        }
        let year = record
            .get(year_col)
            .and_then(|text| text.parse::<u16>().ok());
        let hpi = record
            .get(hpi_col)
            .and_then(|text| text.parse::<f64>().ok());
        if let (Some(year), Some(hpi)) = (year, hpi) {
            series.insert(
                year,
                Point {
                    hpi,
                    annual_change: record
                        .get(change_col)
                        .and_then(|text| text.parse::<f64>().ok()),
                },
            );
        }
    }
    Ok(series)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACT: &str = include_str!("../tests/fixtures/tract.csv");

    #[test]
    fn tract_series_summarizes_newest_year() {
        let series = parse_tract_csv(TRACT.as_bytes(), "01001020100").unwrap();
        let latest = series.latest().unwrap();
        assert_eq!(latest.year, 2012);
        assert!((latest.hpi - 110.0).abs() < 1e-9);
        assert_eq!(latest.change_1y, Some(10.0));
        assert!(
            (latest.change_5y.unwrap() - 10.0).abs() < 1e-9,
            "{latest:?}"
        );
        assert!(
            (latest.change_10y.unwrap() - 10.0).abs() < 1e-9,
            "{latest:?}"
        );
    }

    #[test]
    fn unknown_tract_is_empty() {
        let series = parse_tract_csv(TRACT.as_bytes(), "99999999999").unwrap();
        assert!(series.is_empty());
        assert_eq!(series.latest(), None);
    }

    #[test]
    fn tract_without_header_columns_is_a_format_error() {
        let result = parse_tract_csv("a,b\n1,2\n".as_bytes(), "1");
        assert!(matches!(result, Err(Error::Format(_))));
    }

    #[test]
    fn garbage_workbook_is_a_format_error() {
        let result = parse_xlsx(Level::County, b"not a workbook", "01001");
        assert!(matches!(result, Err(Error::Format(_))));
    }

    #[test]
    fn level_urls_name_the_published_files() {
        assert!(Level::Tract.url().ends_with("/hpi_at_tract.csv"));
        assert!(Level::County.url().ends_with("/hpi_at_county.xlsx"));
    }

    #[test]
    fn zip5_workbook_skips_missing_values_and_other_zips() {
        let bytes = include_bytes!("../tests/fixtures/zip5.xlsx");
        let latest = parse_xlsx(Level::Zip5, bytes, "01001")
            .unwrap()
            .latest()
            .unwrap();
        assert_eq!(latest.year, 2011);
        assert_eq!(latest.change_1y, Some(5.0));
        assert_eq!(latest.change_5y, None);
    }

    #[test]
    fn county_workbook_keys_on_the_fips_column() {
        let bytes = include_bytes!("../tests/fixtures/county.xlsx");
        let series = parse_xlsx(Level::County, bytes, "01003").unwrap();
        assert_eq!(series.latest().unwrap().hpi, 102.0);
        assert!(parse_xlsx(Level::County, bytes, "Autauga")
            .unwrap()
            .is_empty());
    }
}
