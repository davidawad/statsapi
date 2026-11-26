//! Blocking client for the Census Bureau Business Dynamics Statistics (BDS)
//! timeseries API (`api.census.gov/data/timeseries/bds`).
//!
//! Access order follows the source-access ruling: the live API first; an
//! ephemeral, TTL-bounded, size-capped response cache only as the last resort
//! when the live call fails. Nothing else is persisted.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Production endpoint.
pub const BASE_URL: &str = "https://api.census.gov/data/timeseries/bds";
/// Cached responses older than this are never served (7-day bound).
pub const CACHE_TTL: Duration = Duration::from_hours(7 * 24);
/// The cache directory is pruned to at most this many bytes.
pub const CACHE_CAP_BYTES: u64 = 8 * 1024 * 1024;
const ATTEMPTS: u32 = 3;
/// The BDS measures this client reads, in response column order.
pub const MEASURES: [&str; 6] = [
    "ESTABS_ENTRY_RATE",
    "ESTABS_EXIT_RATE",
    "JOB_CREATION_RATE",
    "JOB_DESTRUCTION_RATE",
    "NET_JOB_CREATION_RATE",
    "ESTAB",
];

/// A BDS geography.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Geography {
    Us,
    /// Two-digit state FIPS.
    State(String),
    /// Two-digit state FIPS and three-digit county FIPS.
    County(String, String),
}

/// Where a returned response came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Api,
    Cache,
}

impl Route {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Cache => "cache",
        }
    }
}

#[derive(Debug)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

fn err(message: impl Into<String>) -> Error {
    Error(message.into())
}

/// One year of all-sector (NAICS 00) BDS measures for a geography.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    pub year: u32,
    /// Values in [`MEASURES`] order; `None` where the API suppresses a cell.
    pub values: [Option<f64>; 6],
    pub url: String,
    pub source: Route,
}

impl Observation {
    /// A measure by its BDS variable name.
    #[must_use]
    pub fn get(&self, measure: &str) -> Option<f64> {
        let at = MEASURES.iter().position(|m| *m == measure)?;
        self.values[at]
    }
}

pub struct Client {
    http: reqwest::blocking::Client,
    base: String,
    api_key: Option<String>,
    cache_dir: Option<PathBuf>,
}

impl Client {
    /// `api_key` is the caller's `CENSUS_API_KEY`; `cache_dir` enables the
    /// last-resort fallback cache.
    ///
    /// # Errors
    /// When the HTTP client cannot be built.
    pub fn new(api_key: Option<String>, cache_dir: Option<PathBuf>) -> Result<Self, Error> {
        Self::with_base(BASE_URL, api_key, cache_dir)
    }

    /// As [`Client::new`] against another endpoint (tests).
    ///
    /// # Errors
    /// When the HTTP client cannot be built.
    pub fn with_base(
        base: &str,
        api_key: Option<String>,
        cache_dir: Option<PathBuf>,
    ) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(40))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("statsapi-census-bds/0.1 (+https://github.com/davidawad/statsapi)")
            .build()
            .map_err(|e| err(format!("http client: {e}")))?;
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            api_key: api_key.filter(|k| !k.trim().is_empty()),
            cache_dir,
        })
    }

    /// The newest year with a published all-sector value for `geography`.
    ///
    /// # Errors
    /// On transport, API, or decode failure with no usable cached response.
    pub fn newest(&self, geography: &Geography) -> Result<Observation, Error> {
        let query = query_for(geography);
        let public_url = format!("{}?{query}", self.base);
        let (body, source) = self.fetch(&query)?;
        let mut observation = parse_newest(&body)?;
        observation.url = public_url;
        observation.source = source;
        Ok(observation)
    }

    fn fetch(&self, query: &str) -> Result<(String, Route), Error> {
        match self.live(query) {
            Ok(body) => {
                self.cache_store(query, &body);
                Ok((body, Route::Api))
            }
            Err(live) => self
                .cache_load(query)
                .map(|body| (body, Route::Cache))
                .ok_or(live),
        }
    }

    fn live(&self, query: &str) -> Result<String, Error> {
        let mut url = format!("{}?{query}", self.base);
        if let Some(key) = &self.api_key {
            url.push_str("&key=");
            url.push_str(key);
        }
        let mut last = err("no attempt made");
        for attempt in 0..ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(backoff(attempt));
            }
            match self.http.get(&url).send() {
                Ok(response) => {
                    let status = response.status();
                    if status.is_redirection() {
                        return Err(err(
                            "census api: credential_required: api.census.gov redirected to its missing-key page (set CENSUS_API_KEY)",
                        ));
                    }
                    let text = response
                        .text()
                        .map_err(|e| err(redact(&e.to_string(), self)))?;
                    if status.is_success() {
                        return Ok(text);
                    }
                    last = err(format!("census api: HTTP {status}: {}", snippet(&text)));
                    if status.as_u16() != 429 && !status.is_server_error() {
                        return Err(last);
                    }
                }
                Err(e) => last = err(format!("census api: {}", redact(&e.to_string(), self))),
            }
        }
        Err(last)
    }

    fn cache_path(&self, query: &str) -> Option<PathBuf> {
        let dir = self.cache_dir.as_ref()?;
        let digest = Sha256::digest(format!("{}?{query}", self.base));
        let name = digest.iter().fold(String::new(), |mut acc, b| {
            let _ = write!(acc, "{b:02x}");
            acc
        });
        Some(dir.join(format!("{name}.json")))
    }

    fn cache_load(&self, query: &str) -> Option<String> {
        let path = self.cache_path(query)?;
        let age = SystemTime::now()
            .duration_since(std::fs::metadata(&path).ok()?.modified().ok()?)
            .ok()?;
        (age <= CACHE_TTL)
            .then(|| std::fs::read_to_string(path).ok())
            .flatten()
    }

    fn cache_store(&self, query: &str, body: &str) {
        let (Some(path), Some(dir)) = (self.cache_path(query), self.cache_dir.as_ref()) else {
            return;
        };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        let temp = path.with_extension(format!("tmp{}", std::process::id()));
        if std::fs::write(&temp, body).is_ok() && std::fs::rename(&temp, &path).is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        prune(dir);
    }
}

/// Drop expired entries, then oldest-first until under the size cap.
fn prune(dir: &Path) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(SystemTime, u64, PathBuf)> = read
        .filter_map(Result::ok)
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some((meta.modified().ok()?, meta.len(), e.path()))
        })
        .collect();
    let now = SystemTime::now();
    files.retain(|(modified, _, path)| {
        let expired = now.duration_since(*modified).is_ok_and(|a| a > CACHE_TTL);
        if expired {
            let _ = std::fs::remove_file(path);
        }
        !expired
    });
    files.sort();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    for (_, len, path) in files {
        if total <= CACHE_CAP_BYTES {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total -= len;
        }
    }
}

fn backoff(attempt: u32) -> Duration {
    let jitter = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| u64::from(d.subsec_nanos()) % 400);
    Duration::from_millis(500 * u64::from(attempt) + jitter)
}

fn redact(message: &str, client: &Client) -> String {
    match &client.api_key {
        Some(key) => message.replace(key.as_str(), "<key>"),
        None => message.to_string(),
    }
}

fn snippet(text: &str) -> String {
    text.chars().take(160).collect()
}

/// The query string (without key) for every year of one geography, all
/// sectors. BDS accepts the wildcard `time` predicate for the full series.
#[must_use]
pub fn query_for(geography: &Geography) -> String {
    let scope = match geography {
        Geography::Us => "for=us:1".to_string(),
        Geography::State(state) => format!("for=state:{state}"),
        Geography::County(state, county) => format!("for=county:{county}&in=state:{state}"),
    };
    format!("get=NAME,YEAR,{}&{scope}&NAICS=00", MEASURES.join(","))
}

/// Decode a BDS response (header row then data rows) and return the newest
/// year that carries at least one measure.
///
/// # Errors
/// When the body is not a BDS table or has no usable year.
pub fn parse_newest(body: &str) -> Result<Observation, Error> {
    let table: Value =
        serde_json::from_str(body).map_err(|e| err(format!("census api: not JSON: {e}")))?;
    let rows = table
        .as_array()
        .ok_or_else(|| err("census api: response is not a table"))?;
    let header: Vec<&str> = rows
        .first()
        .and_then(Value::as_array)
        .ok_or_else(|| err("census api: empty table"))?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let column = |name: &str| header.iter().position(|h| *h == name);
    let year_at = column("YEAR").ok_or_else(|| err("census api: no YEAR column"))?;
    let mut best: Option<Observation> = None;
    for row in rows.iter().skip(1).filter_map(Value::as_array) {
        let Some(year) = row
            .get(year_at)
            .and_then(Value::as_str)
            .and_then(|y| y.parse::<u32>().ok())
        else {
            continue;
        };
        let mut values = [None; 6];
        for (slot, measure) in values.iter_mut().zip(MEASURES) {
            *slot = column(measure)
                .and_then(|at| row.get(at))
                .and_then(Value::as_str)
                .and_then(|v| v.parse::<f64>().ok());
        }
        if values.iter().all(Option::is_none) {
            continue;
        }
        if best.as_ref().is_none_or(|b| year > b.year) {
            best = Some(Observation {
                year,
                values,
                url: String::new(),
                source: Route::Api,
            });
        }
    }
    best.ok_or_else(|| err("census api: no year with published BDS values"))
}
