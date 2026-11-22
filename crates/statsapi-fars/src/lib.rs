//! Blocking client for downloading and decoding NHTSA FARS annual CSV archives.
//!
//! ```no_run
//! use statsapi_fars::Client;
//! # fn main() -> Result<(), statsapi_fars::Error> {
//! let client = Client::new();
//! let data = client.fetch_year(2024)?;
//! println!("{} counties", data.counties().len());
//! # Ok(()) }
//! ```

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const RETRIES: u32 = 4;
const ALCOHOL_BAC_THRESHOLD: i64 = 80;
const FATAL_INJURY_CODE: &str = "4";
const PEDESTRIAN_PERSON_TYPE: &str = "5";

/// Errors returned while retrieving or decoding a FARS archive.
#[derive(Debug)]
pub struct Error(String);

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

fn error(message: impl Into<String>) -> Error {
    Error(message.into())
}

/// Fatality totals for one geography.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    /// Fatal people in the geography.
    pub fatalities: u64,
    /// Fatal people in crashes with a driver measured at or above 0.080% BAC.
    pub alcohol_impaired_fatalities: u64,
    /// Pedestrians killed in the geography.
    pub pedestrian_fatalities: u64,
}

/// Decoded FARS data indexed by five-digit county and two-digit state FIPS.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Census {
    counties: BTreeMap<String, Totals>,
    states: BTreeMap<String, Totals>,
}

impl Census {
    /// County totals keyed by five-digit state+county FIPS.
    #[must_use]
    pub fn counties(&self) -> &BTreeMap<String, Totals> {
        &self.counties
    }
    /// State totals keyed by two-digit state FIPS.
    #[must_use]
    pub fn states(&self) -> &BTreeMap<String, Totals> {
        &self.states
    }
}

/// FARS archive client with an optional caller-owned cache directory.
#[derive(Clone, Debug, Default)]
pub struct Client {
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl Client {
    /// Create a client without a cache directory.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the optional caller-owned archive cache directory.
    #[must_use]
    pub fn with_cache_dir(mut self, directory: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(directory.into());
        self
    }

    /// Read only an existing cached archive and fail on a cache miss.
    #[must_use]
    pub fn offline(mut self) -> Self {
        self.offline = true;
        self
    }

    /// Download (or read from cache) and decode an annual national FARS CSV archive.
    ///
    /// # Errors
    ///
    /// Returns an error when the archive cannot be fetched, cached, or decoded.
    pub fn fetch_year(&self, year: u16) -> Result<Census, Error> {
        let url = archive_url(year);
        let cache = self
            .cache_dir
            .as_ref()
            .map(|directory| cache_path(directory, &url));
        if let Some(path) = cache.as_ref().filter(|path| path.exists()) {
            return parse_archive(
                File::open(path).map_err(|e| error(format!("open FARS cache: {e}")))?,
            );
        }
        if self.offline {
            return Err(error(format!("offline cache miss for FARS {year}")));
        }
        let bytes = download(&url)?;
        if let (Some(directory), Some(path)) = (&self.cache_dir, cache) {
            fs::create_dir_all(directory).map_err(|e| error(format!("create FARS cache: {e}")))?;
            let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
            fs::write(&temporary, &bytes).map_err(|e| error(format!("write FARS cache: {e}")))?;
            fs::rename(&temporary, &path).map_err(|e| error(format!("publish FARS cache: {e}")))?;
        }
        parse_archive(Cursor::new(bytes))
    }
}

/// URL of NHTSA's national annual FARS CSV archive.
#[must_use]
pub fn archive_url(year: u16) -> String {
    format!(
        "https://static.nhtsa.gov/nhtsa/downloads/FARS/{year}/National/FARS{year}NationalCSV.zip"
    )
}

fn cache_path(directory: &Path, url: &str) -> PathBuf {
    let digest = Sha256::digest(url.as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as FmtWrite;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    let year = url.split('/').rev().nth(2).unwrap_or("fars");
    directory.join(format!("{year}-{hex}.zip"))
}

fn download(url: &str) -> Result<Vec<u8>, Error> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("statsapi-fars/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|e| error(format!("build NHTSA client: {e}")))?;
    for attempt in 0..=RETRIES {
        match client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
        {
            Ok(response) => {
                return response
                    .bytes()
                    .map(|bytes| bytes.to_vec())
                    .map_err(|e| error(format!("read FARS response: {e}")))
            }
            Err(cause)
                if attempt < RETRIES
                    && (cause.is_timeout()
                        || cause.is_connect()
                        || cause
                            .status()
                            .is_none_or(|s| s.as_u16() == 429 || s.is_server_error())) =>
            {
                let base = 250_u64.saturating_mul(2_u64.pow(attempt));
                let jitter = u64::from(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .subsec_millis(),
                ) % base.max(1);
                thread::sleep(Duration::from_millis(base + jitter));
            }
            Err(cause) => return Err(error(format!("download FARS archive: {cause}"))),
        }
    }
    Err(error("FARS download retries exhausted"))
}

/// Decode an archive reader containing `accident.csv` and `person.csv` entries.
///
/// # Errors
///
/// Returns an error when the ZIP archive is invalid, either table is absent,
/// or a required field or row cannot be decoded.
pub fn parse_archive<R: Read + Seek>(reader: R) -> Result<Census, Error> {
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| error(format!("read FARS zip: {e}")))?;
    let accident_name = find_zip_entry(&mut archive, "accident.csv")?;
    let person_name = find_zip_entry(&mut archive, "person.csv")?;
    let mut accident = Vec::new();
    archive
        .by_name(&accident_name)
        .map_err(|e| error(format!("open {accident_name}: {e}")))?
        .read_to_end(&mut accident)
        .map_err(|e| error(format!("read {accident_name}: {e}")))?;
    let mut person = Vec::new();
    archive
        .by_name(&person_name)
        .map_err(|e| error(format!("open {person_name}: {e}")))?
        .read_to_end(&mut person)
        .map_err(|e| error(format!("read {person_name}: {e}")))?;
    parse_tables(accident.as_slice(), person.as_slice())
}

fn find_zip_entry<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    basename: &str,
) -> Result<String, Error> {
    (0..archive.len())
        .filter_map(|index| {
            archive
                .name_for_index(index)
                .and_then(Result::ok)
                .map(std::borrow::Cow::into_owned)
        })
        .find(|name| {
            name.rsplit('/')
                .next()
                .is_some_and(|entry| entry.eq_ignore_ascii_case(basename))
        })
        .ok_or_else(|| error(format!("FARS archive has no {basename}")))
}

type CaseKey = (String, String);
type CaseFatalities = BTreeMap<CaseKey, (String, u64)>;
type PersonFlags = (BTreeSet<CaseKey>, BTreeMap<CaseKey, u64>);

fn parse_tables<A: Read, P: Read>(accident_csv: A, person_csv: P) -> Result<Census, Error> {
    let (mut census, cases) = parse_accidents(accident_csv)?;
    let (impaired, pedestrians) = parse_people(person_csv)?;
    for (key, (county, fatalities)) in cases {
        if impaired.contains(&key) {
            add(&mut census.states, &key.0, 0, true, fatalities);
            if county != "000" {
                add(
                    &mut census.counties,
                    &format!("{}{county}", key.0),
                    0,
                    true,
                    fatalities,
                );
            }
        }
        let count = pedestrians.get(&key).copied().unwrap_or_default();
        census
            .states
            .entry(key.0.clone())
            .or_default()
            .pedestrian_fatalities += count;
        if county != "000" {
            census
                .counties
                .entry(format!("{}{county}", key.0))
                .or_default()
                .pedestrian_fatalities += count;
        }
    }
    Ok(census)
}

fn parse_accidents<R: Read>(input: R) -> Result<(Census, CaseFatalities), Error> {
    let mut rows = csv::ReaderBuilder::new().flexible(true).from_reader(input);
    let headers = rows
        .headers()
        .map_err(|e| error(format!("FARS accident header: {e}")))?
        .clone();
    let col = |name: &str| {
        headers
            .iter()
            .position(|h| h.eq_ignore_ascii_case(name))
            .ok_or_else(|| error(format!("FARS accident file lacks {name}")))
    };
    let (state_col, county_col, case_col, fatalities_col) = (
        col("STATE")?,
        col("COUNTY")?,
        col("ST_CASE")?,
        col("FATALS")?,
    );
    let mut census = Census::default();
    let mut cases = BTreeMap::new();
    for row in rows.records() {
        let row = row.map_err(|e| error(format!("FARS accident row: {e}")))?;
        let state = pad_code(row.get(state_col).unwrap_or_default(), 2);
        let county = pad_code(row.get(county_col).unwrap_or_default(), 3);
        let id = row.get(case_col).unwrap_or_default().to_owned();
        let fatalities = row
            .get(fatalities_col)
            .unwrap_or_default()
            .parse::<u64>()
            .map_err(|e| error(format!("invalid FARS FATALS for case {id}: {e}")))?;
        let key = (state.clone(), id);
        cases.insert(key, (county.clone(), fatalities));
        add(&mut census.states, &state, fatalities, false, 0);
        if county != "000" {
            add(
                &mut census.counties,
                &format!("{state}{county}"),
                fatalities,
                false,
                0,
            );
        }
    }
    Ok((census, cases))
}

fn parse_people<R: Read>(input: R) -> Result<PersonFlags, Error> {
    let mut rows = csv::ReaderBuilder::new().flexible(true).from_reader(input);
    let headers = rows
        .headers()
        .map_err(|e| error(format!("FARS person header: {e}")))?
        .clone();
    let col = |name: &str| {
        headers
            .iter()
            .position(|h| h.eq_ignore_ascii_case(name))
            .ok_or_else(|| error(format!("FARS person file lacks {name}")))
    };
    let (state_col, case_col, type_col, injury_col, bac_col) = (
        col("STATE")?,
        col("ST_CASE")?,
        col("PER_TYP")?,
        col("INJ_SEV")?,
        col("ALC_RES")?,
    );
    let mut impaired = BTreeSet::new();
    let mut pedestrians = BTreeMap::new();
    for row in rows.records() {
        let row = row.map_err(|e| error(format!("FARS person row: {e}")))?;
        let key = (
            pad_code(row.get(state_col).unwrap_or_default(), 2),
            row.get(case_col).unwrap_or_default().to_owned(),
        );
        let kind = row.get(type_col).unwrap_or_default();
        if kind == "1"
            && row
                .get(bac_col)
                .unwrap_or_default()
                .parse::<i64>()
                .is_ok_and(|bac| (ALCOHOL_BAC_THRESHOLD..=995).contains(&bac))
        {
            impaired.insert(key.clone());
        }
        if kind == PEDESTRIAN_PERSON_TYPE
            && row.get(injury_col).unwrap_or_default() == FATAL_INJURY_CODE
        {
            *pedestrians.entry(key).or_insert(0) += 1;
        }
    }
    Ok((impaired, pedestrians))
}

fn pad_code(value: &str, width: usize) -> String {
    format!("{value:0>width$}")
}
fn add(
    rows: &mut BTreeMap<String, Totals>,
    key: &str,
    fatalities: u64,
    impaired: bool,
    alcohol: u64,
) {
    let totals = rows.entry(key.to_owned()).or_default();
    totals.fatalities += fatalities;
    if impaired {
        totals.alcohol_impaired_fatalities += alcohol;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[allow(clippy::panic_in_result_fn)]
    fn parses_synthetic_upstream_layout_csv() -> Result<(), Box<dyn std::error::Error>> {
        let data = parse_tables(
            include_bytes!("../tests/fixtures/accident.csv").as_slice(),
            include_bytes!("../tests/fixtures/person.csv").as_slice(),
        )?;
        let county = data
            .counties()
            .get("12345")
            .ok_or("missing fixture county")?;
        assert_eq!(
            county,
            &Totals {
                fatalities: 3,
                alcohol_impaired_fatalities: 3,
                pedestrian_fatalities: 1
            }
        );
        assert_eq!(data.states().get("12").map(|row| row.fatalities), Some(5));
        Ok(())
    }
}
