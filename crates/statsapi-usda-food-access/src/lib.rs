//! Reusable access and decoding for USDA ERS food access and FNS SNAP files.

use calamine::{open_workbook_auto_from_rs, Data, Reader};
use reqwest::blocking::Client as HttpClient;
use sha2::{Digest, Sha256};
use std::fmt;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use zip::ZipArchive;

const ERS_ATLAS_URL: &str = "https://www.ers.usda.gov/media/5627/2019-large-retailer-access-map-lram-formerly-known-as-the-food-access-research-atlas-fara-data.zip?v=57565";
const FNS_COUNTY_URL: &str =
    "https://fns-prod.azureedge.us/sites/default/files/resource-files/snap-zip-fns388a-2.zip";
const FNS_PERSONS_URL: &str =
    "https://fns-prod.azureedge.us/sites/default/files/resource-files/snap-persons-11.xlsx";
const FNS_HOUSEHOLDS_URL: &str =
    "https://fns-prod.azureedge.us/sites/default/files/resource-files/snap-households-11.xlsx";
const RETRY_LIMIT: usize = 3;

/// An error returned while downloading or decoding a USDA data file.
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

/// A numeric observation with its source-file vintage and download URL.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Observation {
    /// The decoded numeric source value.
    pub value: f64,
    /// The reporting period or year represented by the value.
    pub vintage: String,
    /// The direct URL of the file used to decode this value.
    pub source_url: String,
}

/// Blocking client for USDA ERS and FNS public data files.
#[derive(Clone)]
pub struct Client {
    http: HttpClient,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl fmt::Debug for Client {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Client")
            .field("cache_dir", &self.cache_dir)
            .field("offline", &self.offline)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Creates a client with an optional caller-owned cache directory.
    ///
    /// Set `USDA_FOOD_ACCESS_OFFLINE=1` to read cached files without network access.
    /// The supported public downloads do not require credentials.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn new(cache_dir: Option<PathBuf>) -> Result<Self, Error> {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(240))
            .build()
            .map_err(|error| Error(error.to_string()))?;
        Ok(Self {
            http,
            cache_dir,
            offline: std::env::var("USDA_FOOD_ACCESS_OFFLINE").as_deref() == Ok("1"),
        })
    }

    /// Creates a client without persistent caching.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn without_cache() -> Result<Self, Error> {
        Self::new(None)
    }

    /// Restricts this client to cache reads, regardless of environment settings.
    #[must_use]
    pub fn offline_only(mut self) -> Self {
        self.offline = true;
        self
    }

    /// Downloads and decodes an ERS Atlas tract field from the 2019 LRAM archive.
    ///
    /// # Errors
    /// Returns an error for transport, HTTP, malformed files, missing tracts, or missing values.
    pub fn atlas_value(&self, tract: &str, field: &str) -> Result<Observation, Error> {
        let bytes = self.fetch(ERS_ATLAS_URL)?;
        let (value, vintage) = decode_atlas(&bytes, tract, field)?;
        Ok(Observation {
            value,
            vintage,
            source_url: ERS_ATLAS_URL.into(),
        })
    }

    /// Downloads and sums the FNS July 2025 SNAP project-area workbook for a county.
    ///
    /// `field` is the workbook's 0-based column index (5 for persons, 11 for households).
    ///
    /// # Errors
    /// Returns an error for transport, HTTP, malformed files, or counties without reported values.
    pub fn county_value(&self, county: &str, field: usize) -> Result<Observation, Error> {
        let bytes = self.fetch(FNS_COUNTY_URL)?;
        let (value, vintage) = decode_county(&bytes, county, field)?;
        Ok(Observation {
            value,
            vintage,
            source_url: FNS_COUNTY_URL.into(),
        })
    }

    /// Downloads and decodes the latest monthly FNS state workbook value.
    ///
    /// `measure` must be `persons` or `households`.
    ///
    /// # Errors
    /// Returns an error for an unknown measure/state, transport, HTTP, or malformed workbook.
    pub fn state_value(&self, measure: &str, state_abbr: &str) -> Result<Observation, Error> {
        let url = match measure {
            "persons" => FNS_PERSONS_URL,
            "households" => FNS_HOUSEHOLDS_URL,
            _ => return Err(Error("measure must be persons or households".into())),
        };
        let bytes = self.fetch(url)?;
        let (value, vintage) = decode_state(&bytes, state_abbr)?;
        Ok(Observation {
            value,
            vintage,
            source_url: url.into(),
        })
    }

    fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        let cache_path = self.cache_dir.as_ref().map(|directory| {
            directory.join(format!("{}.body", hex(&Sha256::digest(url.as_bytes()))))
        });
        if let Some(path) = &cache_path {
            if let Ok(bytes) = std::fs::read(path) {
                return Ok(bytes);
            }
        }
        if self.offline {
            return Err(Error("offline cache miss".into()));
        }

        let mut delay = Duration::from_millis(250);
        for attempt in 0..=RETRY_LIMIT {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    let bytes = response
                        .bytes()
                        .map_err(|error| Error(error.to_string()))?
                        .to_vec();
                    if let Some(path) = cache_path {
                        if let Some(parent) = path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
                        if std::fs::write(&temporary, &bytes).is_ok() {
                            let _ = std::fs::rename(temporary, path);
                        }
                    }
                    return Ok(bytes);
                }
                Ok(response) => {
                    let status = response.status();
                    if !(status.as_u16() == 429 || status.is_server_error())
                        || attempt == RETRY_LIMIT
                    {
                        return Err(Error(format!("USDA HTTP status {status}")));
                    }
                }
                Err(error) if attempt == RETRY_LIMIT => return Err(Error(error.to_string())),
                Err(_) => {}
            }
            let jitter = u64::from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_millis()
                    % 200,
            );
            thread::sleep(delay + Duration::from_millis(jitter));
            delay = (delay * 2).min(Duration::from_secs(4));
        }
        Err(Error("USDA request retry limit reached".into()))
    }
}

fn decode_atlas(bytes: &[u8], tract: &str, field: &str) -> Result<(f64, String), Error> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|error| Error(error.to_string()))?;
    let mut entry = archive
        .by_name("Food Access Research Atlas.csv")
        .map_err(|error| Error(error.to_string()))?;
    let mut reader = csv::Reader::from_reader(&mut entry);
    let headers = reader
        .headers()
        .map_err(|error| Error(error.to_string()))?
        .clone();
    let tract_col = headers
        .iter()
        .position(|header| header == "CensusTract")
        .ok_or("atlas tract field missing")?;
    let value_col = headers
        .iter()
        .position(|header| header == field)
        .ok_or("atlas measure missing")?;
    let requested = tract.trim_start_matches('0');
    for row in reader.records() {
        let row = row.map_err(|error| Error(error.to_string()))?;
        if row
            .get(tract_col)
            .is_some_and(|code| code.trim_start_matches('0') == requested)
        {
            let value = row
                .get(value_col)
                .unwrap_or("")
                .parse::<f64>()
                .map_err(|_| Error(format!("no published value for {field}")))?;
            return Ok((value, "2019".into()));
        }
    }
    Err(Error(format!("tract {tract} absent from 2019 ERS Atlas")))
}

fn decode_state(bytes: &[u8], state_abbr: &str) -> Result<(f64, String), Error> {
    let mut workbook = open_workbook_auto_from_rs(Cursor::new(bytes.to_vec()))
        .map_err(|error| Error(error.to_string()))?;
    let sheet = workbook
        .sheet_names()
        .first()
        .cloned()
        .ok_or("FNS workbook has no sheets")?;
    let range = workbook
        .worksheet_range(&sheet)
        .map_err(|error| Error(error.to_string()))?;
    let rows: Vec<Vec<Data>> = range.rows().map(<[Data]>::to_vec).collect();
    let header = rows
        .iter()
        .position(|row| {
            row.iter()
                .any(|cell| cell.to_string().contains("State / Territory"))
        })
        .ok_or("FNS state header missing")?;
    let state_col = rows[header]
        .iter()
        .position(|cell| cell.to_string().contains("State / Territory"))
        .ok_or("FNS state column missing")?;
    let data_headers: Vec<_> = rows[header]
        .iter()
        .enumerate()
        .skip(state_col + 1)
        .take_while(|(_, cell)| !cell.to_string().contains("Percent Change"))
        .collect();
    let (target_col, date_cell) = data_headers.last().ok_or("FNS monthly columns missing")?;
    let date = date_cell.to_string().replace('\n', " ").trim().to_owned();
    let state_name = state_name(state_abbr).ok_or("unknown state abbreviation")?;
    for row in rows.iter().skip(header + 1) {
        if row.get(state_col).map(ToString::to_string).as_deref() == Some(state_name) {
            let value = row
                .get(*target_col)
                .map(ToString::to_string)
                .ok_or("FNS state value missing")?;
            let number = value
                .replace(',', "")
                .parse::<f64>()
                .map_err(|_| Error("FNS state value is unavailable".into()))?;
            return Ok((number, date));
        }
    }
    Err(Error(format!("FNS state row unavailable: {state_name}")))
}

fn state_name(abbr: &str) -> Option<&'static str> {
    Some(match abbr {
        "AL" => "Alabama",
        "AK" => "Alaska",
        "AZ" => "Arizona",
        "AR" => "Arkansas",
        "CA" => "California",
        "CO" => "Colorado",
        "CT" => "Connecticut",
        "DE" => "Delaware",
        "DC" => "District of Columbia",
        "FL" => "Florida",
        "GA" => "Georgia",
        "HI" => "Hawaii",
        "ID" => "Idaho",
        "IL" => "Illinois",
        "IN" => "Indiana",
        "IA" => "Iowa",
        "KS" => "Kansas",
        "KY" => "Kentucky",
        "LA" => "Louisiana",
        "ME" => "Maine",
        "MD" => "Maryland",
        "MA" => "Massachusetts",
        "MI" => "Michigan",
        "MN" => "Minnesota",
        "MS" => "Mississippi",
        "MO" => "Missouri",
        "MT" => "Montana",
        "NE" => "Nebraska",
        "NV" => "Nevada",
        "NH" => "New Hampshire",
        "NJ" => "New Jersey",
        "NM" => "New Mexico",
        "NY" => "New York",
        "NC" => "North Carolina",
        "ND" => "North Dakota",
        "OH" => "Ohio",
        "OK" => "Oklahoma",
        "OR" => "Oregon",
        "PA" => "Pennsylvania",
        "RI" => "Rhode Island",
        "SC" => "South Carolina",
        "SD" => "South Dakota",
        "TN" => "Tennessee",
        "TX" => "Texas",
        "UT" => "Utah",
        "VT" => "Vermont",
        "VA" => "Virginia",
        "WA" => "Washington",
        "WV" => "West Virginia",
        "WI" => "Wisconsin",
        "WY" => "Wyoming",
        _ => return None,
    })
}

fn decode_county(bytes: &[u8], county: &str, field: usize) -> Result<(f64, String), Error> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|error| Error(error.to_string()))?;
    let file_index = (0..archive.len())
        .find(|index| {
            archive.by_index(*index).is_ok_and(|file| {
                file.name()
                    .is_ok_and(|name| name.eq_ignore_ascii_case("JUL 2025.xlsx"))
            })
        })
        .ok_or("FNS county July 2025 workbook missing")?;
    let mut file = archive
        .by_index(file_index)
        .map_err(|error| Error(error.to_string()))?;
    let mut workbook_bytes = Vec::new();
    file.read_to_end(&mut workbook_bytes)
        .map_err(|error| Error(error.to_string()))?;
    let mut workbook = open_workbook_auto_from_rs(Cursor::new(workbook_bytes))
        .map_err(|error| Error(error.to_string()))?;
    let sheet = workbook
        .sheet_names()
        .first()
        .cloned()
        .ok_or("FNS county workbook has no sheets")?;
    let range = workbook
        .worksheet_range(&sheet)
        .map_err(|error| Error(error.to_string()))?;
    let mut total = 0.0;
    let mut found = false;
    for row in range.rows().skip(3) {
        let raw_code = row
            .first()
            .map_or_default(ToString::to_string)
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned();
        if raw_code.chars().all(|character| character.is_ascii_digit()) {
            let code = format!("{raw_code:0>7}");
            if code.len() >= 5 && &code[..5] == county && !code.ends_with("999") {
                if let Some(value) = row
                    .get(field)
                    .and_then(|cell| cell.to_string().replace(',', "").parse::<f64>().ok())
                {
                    total += value;
                    found = true;
                }
            }
        }
    }
    if found {
        Ok((total, "July 2025".into()))
    } else {
        Err(Error(format!(
            "county {county} not reported by FNS in July 2025"
        )))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            use std::fmt::Write as _;
            let _ = write!(output, "{byte:02x}");
            output
        },
    )
}
