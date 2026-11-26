//! Blocking client for the CDC WONDER XML API (national mortality) and the
//! NCHS mortality datasets on data.cdc.gov (state and county rates).
//!
//! Access order: the live API first; a small, TTL-bounded response cache is
//! read only when the live request fails (or in offline mode). Every result
//! says which path served it through [`AccessPath`].

use reqwest::blocking::Client as HttpClient;
use reqwest::Url;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::fmt;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// CDC WONDER API endpoint for database D76 (Underlying Cause of Death, 1999-2020).
pub const WONDER_URL: &str = "https://wonder.cdc.gov/controller/datarequest/D76";
const SODA: &str = "https://data.cdc.gov/resource";
const LEADING_STATE: &str = "bi63-dtpu";
const DRUG_STATE: &str = "xbxb-epbu";
const DRUG_COUNTY: &str = "rpvx-m2md";
/// WONDER rejects requests closer together than 15 seconds.
const WONDER_GAP: Duration = Duration::from_millis(15_500);
/// Cached responses older than this are never served and are pruned.
pub const CACHE_TTL: Duration = Duration::from_hours(168);
/// Total size the response cache may hold.
pub const CACHE_CAP_BYTES: u64 = 4 * 1024 * 1024;

/// A national cause of death the WONDER API can group by year.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cause {
    /// ICD-10 X60-X84, intentional self-harm.
    Suicide,
    /// Drug-induced causes D1-D4 (poisoning overdose of every intent).
    DrugOverdose,
    /// Alcohol-induced causes (A9).
    AlcoholInduced,
    /// Malignant neoplasms, ICD-10 C00-C97.
    Cancer,
}

impl Cause {
    /// Parses the short name used in catalog definitions.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "suicide" => Some(Self::Suicide),
            "drug_overdose" => Some(Self::DrugOverdose),
            "alcohol_induced" => Some(Self::AlcoholInduced),
            "cancer" => Some(Self::Cancer),
            _ => None,
        }
    }

    /// WONDER filter variable, the value codes, and the variable the
    /// `O_ucd` option must point at.
    fn filter(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Suicide => ("V2", &["X60-X84"]),
            Self::DrugOverdose => ("V25", &["D1", "D2", "D3", "D4"]),
            Self::AlcoholInduced => ("V25", &["A9"]),
            Self::Cancer => ("V2", &["C00-C97"]),
        }
    }
}

/// Which path served a response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessPath {
    /// A live API request.
    Api,
    /// The TTL-bounded fallback cache.
    Cache,
}

impl AccessPath {
    /// The value recorded in `measurement.source` paths: `api` or `cache`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Cache => "cache",
        }
    }
}

/// A decoded value with the request that produced it.
#[derive(Clone, Debug)]
pub struct Fetched<T> {
    /// The URL (with query) that was read, for attribution.
    pub url: String,
    /// Whether the value came from the live API or the cache.
    pub path: AccessPath,
    /// The decoded value.
    pub value: T,
}

/// One year of a WONDER national table.
#[derive(Clone, Debug, PartialEq)]
pub struct UcdRow {
    /// Calendar year.
    pub year: i32,
    /// Death count when published (not suppressed).
    pub deaths: Option<u64>,
    /// Population estimate.
    pub population: Option<u64>,
    /// Crude rate per 100,000.
    pub crude_rate: Option<f64>,
    /// Age-adjusted rate per 100,000 (2000 US standard population).
    pub age_adjusted_rate: Option<f64>,
    /// Lower 95% confidence limit of the age-adjusted rate.
    pub ci_low: Option<f64>,
    /// Upper 95% confidence limit of the age-adjusted rate.
    pub ci_high: Option<f64>,
    /// Standard error of the age-adjusted rate.
    pub std_error: Option<f64>,
}

/// One age-adjusted rate from a data.cdc.gov dataset.
#[derive(Clone, Debug, PartialEq)]
pub struct RateRow {
    /// Calendar year.
    pub year: i32,
    /// Age-adjusted (or model-based) death rate per 100,000.
    pub rate: f64,
    /// Lower 95% interval, when the dataset publishes one.
    pub ci_low: Option<f64>,
    /// Upper 95% interval, when the dataset publishes one.
    pub ci_high: Option<f64>,
    /// Death count, when the dataset publishes one.
    pub deaths: Option<u64>,
}

/// Errors returned while building or fetching a response.
#[derive(Debug)]
#[non_exhaustive]
pub enum ClientError {
    /// A query value is malformed.
    InvalidQuery(String),
    /// An HTTP request failed after retries.
    Http(reqwest::Error),
    /// The upstream answered with a non-success status.
    Status(u16),
    /// WONDER answered with a processing error message.
    Wonder(String),
    /// The response could not be decoded.
    Decode(String),
    /// Offline mode (or a failed live request) found no fresh cache entry.
    CacheMiss(PathBuf),
    /// A cache operation failed.
    Cache(std::io::Error),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQuery(message) => write!(f, "invalid query: {message}"),
            Self::Http(error) => write!(f, "CDC request failed: {error}"),
            Self::Status(code) => write!(f, "CDC request returned HTTP {code}"),
            Self::Wonder(message) => write!(f, "CDC WONDER error: {message}"),
            Self::Decode(message) => write!(f, "undecodable CDC response: {message}"),
            Self::CacheMiss(path) => write!(f, "no fresh cache entry: {}", path.display()),
            Self::Cache(error) => write!(f, "CDC cache failed: {error}"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Cache(error) => Some(error),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<std::io::Error> for ClientError {
    fn from(error: std::io::Error) -> Self {
        Self::Cache(error)
    }
}

/// A blocking CDC client. It is single-threaded by design: the WONDER request
/// spacing is tracked per client.
#[derive(Clone, Debug)]
pub struct WonderClient {
    http: HttpClient,
    cache_dir: Option<PathBuf>,
    offline: bool,
    max_retries: usize,
    last_wonder: Cell<Option<Instant>>,
}

impl WonderClient {
    /// Creates a live client with no cache and three retries.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be initialized.
    pub fn new() -> Result<Self, ClientError> {
        Ok(Self {
            http: HttpClient::builder()
                // wonder.cdc.gov's edge answers 403 unless the user agent is
                // browser-shaped ("Mozilla/5.0 (compatible; product; +url)").
                .user_agent(concat!(
                    "Mozilla/5.0 (compatible; statsapi-cdc-wonder/",
                    env!("CARGO_PKG_VERSION"),
                    "; +https://github.com/davidawad/statsapi)"
                ))
                .timeout(Duration::from_secs(120))
                .build()?,
            cache_dir: None,
            offline: false,
            max_retries: 3,
            last_wonder: Cell::new(None),
        })
    }

    /// Enables the fallback cache under a caller-supplied directory. Entries
    /// expire after [`CACHE_TTL`] and the directory is held under
    /// [`CACHE_CAP_BYTES`].
    #[must_use]
    pub fn with_cache_dir(mut self, directory: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(directory.as_ref().to_path_buf());
        self
    }

    /// Makes requests read only from the fallback cache.
    #[must_use]
    pub fn offline(mut self) -> Self {
        self.offline = true;
        self
    }

    /// Sets the number of retries after the initial attempt.
    #[must_use]
    pub fn with_max_retries(mut self, retries: usize) -> Self {
        self.max_retries = retries;
        self
    }

    /// Reads the national table for one cause, one row per year (1999-2020).
    ///
    /// # Errors
    ///
    /// Returns an error when the request fails and no fresh cache entry
    /// exists, or when the response cannot be decoded.
    pub fn national_series(&self, cause: Cause) -> Result<Fetched<Vec<UcdRow>>, ClientError> {
        let request = wonder_request_xml(cause);
        let key = format!("{WONDER_URL}\n{request}");
        let fetched = self.cached(&key, || self.post_wonder(&request))?;
        let rows = parse_wonder_table(&fetched.1)?;
        Ok(Fetched {
            url: WONDER_URL.to_string(),
            path: fetched.0,
            value: rows,
        })
    }

    /// Newest leading-cause age-adjusted rate for a state (NCHS
    /// "Leading Causes of Death", dataset `bi63-dtpu`). `cause_name` is the
    /// dataset's short name such as `Suicide` or `Heart disease`.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed names, failed requests, or an empty
    /// result.
    pub fn leading_cause_state(
        &self,
        state_name: &str,
        cause_name: &str,
    ) -> Result<Fetched<RateRow>, ClientError> {
        let filter = format!(
            "state='{}' AND cause_name='{}'",
            soql(state_name)?,
            soql(cause_name)?
        );
        self.soda_row(LEADING_STATE, &filter, |row| {
            Some(RateRow {
                year: number(row, "year")?,
                rate: number(row, "aadr")?,
                ci_low: None,
                ci_high: None,
                deaths: number(row, "deaths"),
            })
        })
    }

    /// Newest age-adjusted drug-poisoning rate for a state (dataset
    /// `xbxb-epbu`, all ages, both sexes, all races).
    ///
    /// # Errors
    ///
    /// Returns an error for malformed names, failed requests, or an empty
    /// result.
    pub fn drug_poisoning_state(&self, state_name: &str) -> Result<Fetched<RateRow>, ClientError> {
        let filter = format!(
            "state='{}' AND sex='Both Sexes' AND age_group='All Ages' AND race_and_hispanic_origin='All Races-All Origins'",
            soql(state_name)?
        );
        self.soda_row(DRUG_STATE, &filter, |row| {
            Some(RateRow {
                year: number(row, "year")?,
                rate: number(row, "age_adjusted_rate")?,
                ci_low: number(row, "lower_confidence_limit_for_age_adjusted_rate"),
                ci_high: number(row, "upper_confidence_limit_for_age_adjusted_rate"),
                deaths: number(row, "deaths"),
            })
        })
    }

    /// Newest model-based drug-poisoning rate for a county (dataset
    /// `rpvx-m2md`), keyed by the 5-digit county FIPS code.
    ///
    /// # Errors
    ///
    /// Returns an error for a malformed FIPS code, failed requests, or an
    /// empty result.
    pub fn drug_poisoning_county(&self, county5: &str) -> Result<Fetched<RateRow>, ClientError> {
        if county5.len() != 5 || !county5.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(ClientError::InvalidQuery(
                "county FIPS must be 5 digits".into(),
            ));
        }
        // The dataset stores FIPS without the leading zero ("1001").
        let filter = format!("fips='{}'", county5.trim_start_matches('0'));
        self.soda_row(DRUG_COUNTY, &filter, |row| {
            Some(RateRow {
                year: number(row, "year")?,
                rate: number(row, "model_based_death_rate")?,
                ci_low: number(row, "lower95ci"),
                ci_high: number(row, "upper95ci"),
                deaths: None,
            })
        })
    }

    fn soda_row(
        &self,
        dataset: &str,
        filter: &str,
        build: impl Fn(&Value) -> Option<RateRow>,
    ) -> Result<Fetched<RateRow>, ClientError> {
        let mut url = Url::parse(&format!("{SODA}/{dataset}.json"))
            .map_err(|error| ClientError::InvalidQuery(error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("$where", filter)
            .append_pair("$order", "year DESC")
            .append_pair("$limit", "1");
        let url = String::from(url);
        let (path, body) = self.cached(&url, || self.get_with_retry(&url))?;
        let payload: Value =
            serde_json::from_str(&body).map_err(|error| ClientError::Decode(error.to_string()))?;
        let row = payload
            .as_array()
            .and_then(|rows| rows.first())
            .and_then(build)
            .ok_or_else(|| ClientError::Decode(format!("no usable row from {dataset}")))?;
        Ok(Fetched {
            url,
            path,
            value: row,
        })
    }

    /// Live first; on failure fall back to a fresh cache entry; offline reads
    /// only the cache. A successful live body refreshes the cache.
    fn cached(
        &self,
        key: &str,
        live: impl FnOnce() -> Result<String, ClientError>,
    ) -> Result<(AccessPath, String), ClientError> {
        let path = self
            .cache_dir
            .as_ref()
            .map(|directory| directory.join(cache_key(key)));
        if !self.offline {
            match live() {
                Ok(body) => {
                    if let (Some(path), Some(directory)) = (path.as_ref(), self.cache_dir.as_ref())
                    {
                        // Best effort: a cache write failure must not fail a live read.
                        let _ = write_cache(path, &body);
                        let _ = prune_cache(directory);
                    }
                    return Ok((AccessPath::Api, body));
                }
                Err(error) => {
                    return match path.as_deref().and_then(read_fresh) {
                        Some(body) => Ok((AccessPath::Cache, body)),
                        None => Err(error),
                    };
                }
            }
        }
        let path = path.unwrap_or_default();
        read_fresh(&path)
            .map(|body| (AccessPath::Cache, body))
            .ok_or(ClientError::CacheMiss(path))
    }

    fn get_with_retry(&self, url: &str) -> Result<String, ClientError> {
        let mut last = ClientError::Status(0);
        for attempt in 0..=self.max_retries {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    return Ok(response.text()?);
                }
                Ok(response) if !retryable_status(response.status().as_u16()) => {
                    return Err(ClientError::Status(response.status().as_u16()));
                }
                Ok(response) => last = ClientError::Status(response.status().as_u16()),
                Err(error) => last = ClientError::Http(error),
            }
            if attempt < self.max_retries {
                jittered_delay(attempt);
            }
        }
        Err(last)
    }

    fn post_wonder(&self, request_xml: &str) -> Result<String, ClientError> {
        let mut last = ClientError::Status(0);
        for attempt in 0..=self.max_retries {
            if let Some(previous) = self.last_wonder.get() {
                if let Some(wait) = WONDER_GAP.checked_sub(previous.elapsed()) {
                    std::thread::sleep(wait);
                }
            }
            self.last_wonder.set(Some(Instant::now()));
            let sent = self
                .http
                .post(WONDER_URL)
                .form(&[("request_xml", request_xml)])
                .send();
            match sent {
                Ok(response) if response.status().is_success() => {
                    let body = response.text()?;
                    match wonder_error(&body) {
                        // Another process used the shared 15 s window: wait and retry.
                        Some(message) if message.starts_with("Request rate exceeded") => {
                            last = ClientError::Wonder(message);
                        }
                        Some(message) => return Err(ClientError::Wonder(message)),
                        None => return Ok(body),
                    }
                }
                Ok(response) if !retryable_status(response.status().as_u16()) => {
                    return Err(ClientError::Status(response.status().as_u16()));
                }
                Ok(response) => last = ClientError::Status(response.status().as_u16()),
                Err(error) => last = ClientError::Http(error),
            }
            if attempt < self.max_retries {
                jittered_delay(attempt);
            }
        }
        Err(last)
    }
}

/// Maps a USPS state abbreviation to the name data.cdc.gov datasets use.
#[must_use]
pub fn state_name(abbr: &str) -> Option<&'static str> {
    const STATES: [(&str, &str); 51] = [
        ("AL", "Alabama"),
        ("AK", "Alaska"),
        ("AZ", "Arizona"),
        ("AR", "Arkansas"),
        ("CA", "California"),
        ("CO", "Colorado"),
        ("CT", "Connecticut"),
        ("DE", "Delaware"),
        ("DC", "District of Columbia"),
        ("FL", "Florida"),
        ("GA", "Georgia"),
        ("HI", "Hawaii"),
        ("ID", "Idaho"),
        ("IL", "Illinois"),
        ("IN", "Indiana"),
        ("IA", "Iowa"),
        ("KS", "Kansas"),
        ("KY", "Kentucky"),
        ("LA", "Louisiana"),
        ("ME", "Maine"),
        ("MD", "Maryland"),
        ("MA", "Massachusetts"),
        ("MI", "Michigan"),
        ("MN", "Minnesota"),
        ("MS", "Mississippi"),
        ("MO", "Missouri"),
        ("MT", "Montana"),
        ("NE", "Nebraska"),
        ("NV", "Nevada"),
        ("NH", "New Hampshire"),
        ("NJ", "New Jersey"),
        ("NM", "New Mexico"),
        ("NY", "New York"),
        ("NC", "North Carolina"),
        ("ND", "North Dakota"),
        ("OH", "Ohio"),
        ("OK", "Oklahoma"),
        ("OR", "Oregon"),
        ("PA", "Pennsylvania"),
        ("RI", "Rhode Island"),
        ("SC", "South Carolina"),
        ("SD", "South Dakota"),
        ("TN", "Tennessee"),
        ("TX", "Texas"),
        ("UT", "Utah"),
        ("VT", "Vermont"),
        ("VA", "Virginia"),
        ("WA", "Washington"),
        ("WV", "West Virginia"),
        ("WI", "Wisconsin"),
        ("WY", "Wyoming"),
    ];
    STATES
        .iter()
        .find(|(code, _)| code.eq_ignore_ascii_case(abbr))
        .map(|(_, name)| *name)
}

/// Builds the WONDER request document for one national by-year table.
#[must_use]
pub fn wonder_request_xml(cause: Cause) -> String {
    let (variable, codes) = cause.filter();
    let mut params: Vec<(String, Vec<String>)> = Vec::new();
    let mut add = |name: &str, values: &[&str]| {
        params.push((
            name.to_string(),
            values.iter().map(|value| (*value).to_string()).collect(),
        ));
    };
    add("accept_datause_restrictions", &["true"]);
    add("B_1", &["D76.V1-level1"]);
    for slot in ["B_2", "B_3", "B_4", "B_5"] {
        add(slot, &["*None*"]);
    }
    for filter in ["V1", "V9", "V10", "V25", "V27", "V2"] {
        if filter == variable {
            add(&format!("F_D76.{filter}"), codes);
        } else {
            add(&format!("F_D76.{filter}"), &["*All*"]);
        }
    }
    for (name, value) in [
        ("M_1", "D76.M1"),
        ("M_2", "D76.M2"),
        ("M_3", "D76.M3"),
        ("O_aar", "aar_std"),
        ("O_aar_CI", "true"),
        ("O_aar_SE", "true"),
        ("O_aar_enable", "true"),
        ("O_aar_pop", "0000"),
        ("O_age", "D76.V5"),
        ("O_javascript", "off"),
        ("O_location", "D76.V9"),
        ("O_precision", "1"),
        ("O_rate_per", "100000"),
        ("O_show_suppressed", "true"),
        ("O_show_totals", "false"),
        ("O_show_zeros", "true"),
        ("O_timeout", "600"),
        ("O_title", "statsapi-cdc-wonder"),
        ("O_urban", "D76.V19"),
        ("V_D76.V6", "00"),
    ] {
        add(name, &[value]);
    }
    let ucd = format!("D76.{variable}");
    add("O_ucd", &[ucd.as_str()]);
    for field in ["V1", "V2", "V9", "V10", "V25", "V27"] {
        add(&format!("O_{field}_fmode"), &["freg"]);
        add(&format!("V_D76.{field}"), &[""]);
    }
    for field in [
        "V11", "V12", "V17", "V19", "V20", "V21", "V22", "V23", "V24", "V4", "V5", "V51", "V52",
        "V7", "V8",
    ] {
        add(&format!("V_D76.{field}"), &["*All*"]);
    }
    let mut xml = String::from("<request-parameters>\n");
    for (name, values) in &params {
        let _ = write!(xml, "<parameter><name>{name}</name>");
        for value in values {
            let _ = write!(xml, "<value>{value}</value>");
        }
        xml.push_str("</parameter>\n");
    }
    xml.push_str("</request-parameters>\n");
    xml
}

/// Returns WONDER's processing-error message, if the document is one.
#[must_use]
pub fn wonder_error(body: &str) -> Option<String> {
    if !body.contains("<title>Processing Error</title>") {
        return None;
    }
    let start = body.find("<message")?;
    let text = &body[body[start..].find('>')? + start + 1..];
    let text = text.split("</message>").next()?;
    let text = text.trim().trim_start_matches("<![CDATA[");
    Some(text.trim_end_matches("]]>").trim().to_string())
}

/// Decodes a WONDER results document into one row per year.
///
/// The table columns are, left to right: year, deaths, population, crude rate,
/// age-adjusted rate (with a confidence-interval child), and its standard error.
///
/// # Errors
///
/// Returns [`ClientError::Wonder`] for a processing error and
/// [`ClientError::Decode`] when the document holds no data rows.
pub fn parse_wonder_table(body: &str) -> Result<Vec<UcdRow>, ClientError> {
    if let Some(message) = wonder_error(body) {
        return Err(ClientError::Wonder(message));
    }
    let table = body
        .split("<data-table")
        .nth(1)
        .and_then(|rest| rest.split("</data-table>").next())
        .ok_or_else(|| ClientError::Decode("no data-table in WONDER response".into()))?;
    let mut rows = Vec::new();
    for row in table.split("<r>").skip(1) {
        let row = row.split("</r>").next().unwrap_or_default();
        let mut cells = row.split("<c ").skip(1);
        let Some(year) = cells
            .next()
            .and_then(|cell| attribute(cell, "l"))
            .and_then(|label| label.parse::<i32>().ok())
        else {
            continue;
        };
        let cells: Vec<&str> = cells.collect();
        let value = |index: usize| cells.get(index).and_then(|cell| attribute(cell, "v"));
        let (ci_low, ci_high) = cells
            .get(3)
            .and_then(|cell| attribute_after(cell, "<l v=\""))
            .and_then(|range| {
                let range = range.trim_matches(|c| c == '(' || c == ')');
                let (low, high) = range.split_once(" - ")?;
                Some((low.trim().parse().ok(), high.trim().parse().ok()))
            })
            .unwrap_or((None, None));
        rows.push(UcdRow {
            year,
            deaths: value(0).and_then(|text| count(&text)),
            population: value(1).and_then(|text| count(&text)),
            crude_rate: value(2).and_then(|text| text.parse().ok()),
            age_adjusted_rate: value(3).and_then(|text| text.parse().ok()),
            ci_low,
            ci_high,
            std_error: value(4).and_then(|text| text.parse().ok()),
        });
    }
    if rows.is_empty() {
        return Err(ClientError::Decode("WONDER table has no rows".into()));
    }
    Ok(rows)
}

/// Selects the newest row that has an age-adjusted rate.
#[must_use]
pub fn latest_row(rows: &[UcdRow]) -> Option<&UcdRow> {
    rows.iter()
        .filter(|row| row.age_adjusted_rate.is_some())
        .max_by_key(|row| row.year)
}

fn attribute(cell: &str, name: &str) -> Option<String> {
    let cell = cell.split('>').next().unwrap_or(cell);
    attribute_after(cell, &format!("{name}=\""))
}

fn attribute_after(text: &str, marker: &str) -> Option<String> {
    let start = text.find(marker)? + marker.len();
    Some(text[start..].split('"').next()?.to_string())
}

fn count(text: &str) -> Option<u64> {
    text.replace(',', "").parse().ok()
}

fn number<T: std::str::FromStr>(row: &Value, field: &str) -> Option<T> {
    match row.get(field)? {
        Value::String(text) => text.trim().parse().ok(),
        Value::Number(number) => number.to_string().parse().ok(),
        _ => None,
    }
}

fn soql(value: &str) -> Result<String, ClientError> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(ClientError::InvalidQuery("empty or control value".into()));
    }
    Ok(value.replace('\'', "''"))
}

fn retryable_status(status: u16) -> bool {
    status == 429 || (500..=599).contains(&status)
}

fn jittered_delay(attempt: usize) {
    let base_ms = 200_u64.saturating_mul(1_u64 << attempt.min(5));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    std::thread::sleep(Duration::from_millis(
        base_ms / 2 + u64::from(nanos) % base_ms,
    ));
}

fn cache_key(key: &str) -> String {
    Sha256::digest(key.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut digest, byte| {
            let _ = write!(digest, "{byte:02x}");
            digest
        })
}

fn read_fresh(path: &Path) -> Option<String> {
    let age = std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .elapsed()
        .ok()?;
    if age > CACHE_TTL {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn write_cache(path: &Path, body: &str) -> Result<(), ClientError> {
    std::fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(".")))?;
    let temp = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&temp, body)?;
    std::fs::rename(temp, path)?;
    Ok(())
}

/// Removes expired entries, then the oldest until the directory fits the cap.
fn prune_cache(directory: &Path) -> Result<(), ClientError> {
    let mut entries: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if !meta.is_file() {
            continue;
        }
        let modified = meta.modified().unwrap_or(UNIX_EPOCH);
        if modified.elapsed().is_ok_and(|age| age > CACHE_TTL) {
            std::fs::remove_file(entry.path())?;
            continue;
        }
        entries.push((modified, meta.len(), entry.path()));
    }
    entries.sort();
    let mut total: u64 = entries.iter().map(|entry| entry.1).sum();
    for (_, size, path) in entries {
        if total <= CACHE_CAP_BYTES {
            break;
        }
        std::fs::remove_file(path)?;
        total -= size;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUICIDE: &str = include_str!("../tests/fixtures/ucd-suicide.xml");

    #[test]
    fn parses_year_rows_with_confidence_interval() {
        let rows = parse_wonder_table(SUICIDE).expect("fixture parses");
        assert_eq!(rows.len(), 3);
        let newest = latest_row(&rows).expect("newest row");
        assert_eq!(newest.year, 2020);
        assert_eq!(newest.deaths, Some(3456));
        assert_eq!(newest.population, Some(302_000_000));
        assert_eq!(newest.age_adjusted_rate, Some(3.25));
        assert_eq!(newest.ci_low, Some(3.1));
        assert_eq!(newest.ci_high, Some(3.4));
        assert_eq!(newest.std_error, Some(0.3));
    }

    #[test]
    fn rate_limit_message_is_reported() {
        let body = include_str!("../tests/fixtures/ucd-rate-limited.xml");
        assert!(wonder_error(body)
            .expect("error message")
            .starts_with("Request rate exceeded"));
        assert!(matches!(
            parse_wonder_table(body),
            Err(ClientError::Wonder(_))
        ));
    }

    #[test]
    fn request_filters_the_cause_variable() {
        let drug = wonder_request_xml(Cause::DrugOverdose);
        assert!(drug.contains(
            "<name>F_D76.V25</name><value>D1</value><value>D2</value><value>D3</value><value>D4</value>"
        ));
        assert!(drug.contains("<name>O_ucd</name><value>D76.V25</value>"));
        let suicide = wonder_request_xml(Cause::Suicide);
        assert!(suicide.contains("<name>F_D76.V2</name><value>X60-X84</value>"));
        assert!(suicide.contains("<name>accept_datause_restrictions</name><value>true</value>"));
    }

    #[test]
    fn state_names_resolve_from_abbreviations() {
        assert_eq!(state_name("nj"), Some("New Jersey"));
        assert_eq!(state_name("ZZ"), None);
    }

    #[test]
    fn soda_values_are_quoted_and_validated() {
        assert_eq!(soql("O'Brien").expect("quoted"), "O''Brien");
        assert!(soql("").is_err());
        let client = WonderClient::new().expect("client");
        assert!(client.drug_poisoning_county("3402").is_err());
    }

    #[test]
    fn rate_rows_decode_from_dataset_fixtures() {
        let county: Value =
            serde_json::from_str(include_str!("../tests/fixtures/drug-county.json"))
                .expect("fixture");
        let row = &county[0];
        assert_eq!(number::<f64>(row, "model_based_death_rate"), Some(12.5));
        assert_eq!(number::<i32>(row, "year"), Some(2021));
    }

    #[test]
    fn cache_is_a_fallback_and_expires() {
        let directory =
            std::env::temp_dir().join(format!("statsapi-cdc-test-{}", std::process::id()));
        let client = WonderClient::new()
            .expect("client")
            .with_cache_dir(&directory);
        let path = directory.join(cache_key("k"));
        write_cache(&path, "cached").expect("write");
        // A failing live request falls back to the fresh entry.
        let (via, body) = client
            .cached("k", || Err(ClientError::Status(503)))
            .expect("fallback");
        assert_eq!((via, body.as_str()), (AccessPath::Cache, "cached"));
        // A succeeding live request wins and refreshes the entry.
        let (via, body) = client
            .cached("k", || Ok("fresh".to_string()))
            .expect("live");
        assert_eq!((via, body.as_str()), (AccessPath::Api, "fresh"));
        assert_eq!(read_fresh(&path).as_deref(), Some("fresh"));
        // Offline with no entry is a cache miss.
        let offline = client.offline();
        assert!(matches!(
            offline.cached("absent", || Ok(String::new())),
            Err(ClientError::CacheMiss(_))
        ));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    #[ignore = "live CDC WONDER and data.cdc.gov smoke test"]
    fn live_national_state_and_county_reads() {
        let client = WonderClient::new().expect("client");
        let national = client
            .national_series(Cause::Suicide)
            .expect("WONDER response");
        assert_eq!(national.path, AccessPath::Api);
        assert!(latest_row(&national.value).is_some());
        let state = client
            .leading_cause_state("New Jersey", "Suicide")
            .expect("state rate");
        assert!(state.value.rate > 0.0);
        let county = client.drug_poisoning_county("34023").expect("county rate");
        assert!(county.value.rate > 0.0);
    }
}
