//! Blocking access to the V-Dem Country-Year dataset.

use std::fmt;
use std::io::{Read, Seek};
use std::time::Duration;

/// Current V-Dem Country-Year Core archive endpoint.
pub const DATASET_URL: &str = "https://www.v-dem.net/media/datasets/V-Dem-CY-Core-v16_csv.zip";

/// Adapter-facing V-Dem metric names and their CSV columns.
pub const COLUMNS: [(&str, &str); 5] = [
    ("liberal_democracy_index", "v2x_libdem"),
    ("electoral_democracy_index", "v2x_polyarchy"),
    ("freedom_of_expression_index", "v2x_freexp_altinf"),
    ("civil_society_participation_index", "v2x_cspart"),
    ("political_corruption_index", "v2x_corr"),
];

/// Errors returned while fetching or parsing a V-Dem extract.
#[derive(Debug)]
pub enum Error {
    /// The archive request failed.
    Http(reqwest::Error),
    /// The archive is not a valid ZIP file.
    Zip(zip::result::ZipError),
    /// The CSV could not be read or written.
    Csv(csv::Error),
    /// A required column was absent.
    MissingColumn(String),
    /// No CSV member was found in the archive.
    MissingCsv,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(error) => write!(formatter, "V-Dem HTTP request failed: {error}"),
            Self::Zip(error) => write!(formatter, "V-Dem archive is invalid: {error}"),
            Self::Csv(error) => write!(formatter, "V-Dem CSV is invalid: {error}"),
            Self::MissingColumn(column) => write!(formatter, "V-Dem CSV has no {column}"),
            Self::MissingCsv => formatter.write_str("V-Dem archive contains no CSV"),
        }
    }
}

impl std::error::Error for Error {}

/// A blocking V-Dem client.
#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
}

impl Client {
    /// Creates a client with a bounded archive request timeout.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be initialized.
    pub fn new() -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!("statsapi-vdem/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(90))
            .build()
            .map_err(Error::Http)?;
        Ok(Self { http })
    }

    /// Downloads and extracts the small CSV projection used by adapters.
    ///
    /// # Errors
    ///
    /// Returns an error when the archive request or extraction fails.
    pub fn fetch_extract(&self) -> Result<Vec<u8>, Error> {
        let archive = self
            .http
            .get(DATASET_URL)
            .send()
            .map_err(Error::Http)?
            .error_for_status()
            .map_err(Error::Http)?
            .bytes()
            .map_err(Error::Http)?;
        extract_archive(std::io::Cursor::new(archive))
    }
}

/// Extracts the adapter columns from a V-Dem ZIP archive.
///
/// # Errors
///
/// Returns an error when the archive, CSV, or required columns are invalid.
pub fn extract_archive<R: Read + Seek>(archive: R) -> Result<Vec<u8>, Error> {
    let mut zip = zip::ZipArchive::new(archive).map_err(Error::Zip)?;
    for index in 0..zip.len() {
        let file = zip.by_index(index).map_err(Error::Zip)?;
        if !file.name().map_err(Error::Zip)?.ends_with(".csv") {
            continue;
        }
        let mut reader = csv::Reader::from_reader(file);
        let headers = reader.headers().map_err(Error::Csv)?.clone();
        let wanted: Vec<&str> = ["country_text_id", "year"]
            .into_iter()
            .chain(COLUMNS.iter().map(|(_, column)| *column))
            .collect();
        let indexes = wanted
            .iter()
            .map(|name| {
                headers
                    .iter()
                    .position(|header| header == *name)
                    .ok_or_else(|| Error::MissingColumn((*name).to_owned()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut writer = csv::Writer::from_writer(Vec::new());
        writer.write_record(&wanted).map_err(Error::Csv)?;
        for row in reader.records() {
            let row = row.map_err(Error::Csv)?;
            writer
                .write_record(indexes.iter().map(|&index| row.get(index).unwrap_or("")))
                .map_err(Error::Csv)?;
        }
        return writer
            .into_inner()
            .map_err(|error| Error::Csv(error.into_error().into()));
    }
    Err(Error::MissingCsv)
}

/// Returns the latest numeric value for one country and CSV column.
///
/// # Errors
///
/// Returns an error when the CSV is invalid or a required column is absent.
pub fn latest(csv: &[u8], country: &str, column: &str) -> Result<Option<(f64, String)>, Error> {
    let mut reader = csv::Reader::from_reader(csv);
    let headers = reader.headers().map_err(Error::Csv)?.clone();
    let index = |name: &str| {
        headers
            .iter()
            .position(|value| value == name)
            .ok_or_else(|| Error::MissingColumn(name.to_owned()))
    };
    let name_index = index("country_text_id")?;
    let year_index = index("year")?;
    let value_index = index(column)?;
    let mut latest = None;
    for row in reader.records() {
        let row = row.map_err(Error::Csv)?;
        if row.get(name_index) != Some(country) {
            continue;
        }
        let Some(year) = row
            .get(year_index)
            .and_then(|value| value.parse::<i32>().ok())
        else {
            continue;
        };
        let Some(value) = row
            .get(value_index)
            .and_then(|value| value.parse::<f64>().ok())
        else {
            continue;
        };
        if latest.is_none_or(|(prior, _): (i32, f64)| year > prior) {
            latest = Some((year, value));
        }
    }
    Ok(latest.map(|(year, value)| (value, year.to_string())))
}
