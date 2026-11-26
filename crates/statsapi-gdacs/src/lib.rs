//! Blocking client for the keyless GDACS event-list API.
//!
//! Source order: the live API first; an optional, TTL-bounded, size-capped
//! cache directory is read only when the live request fails.

use chrono::NaiveDate;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BASE: &str = "https://www.gdacs.org/gdacsapi/api/events/geteventlist/SEARCH";
const MAX_RETRIES: u32 = 3;
/// Longest a cached response may be served; the repo rule is seven days.
pub const MAX_CACHE_TTL: Duration = Duration::from_hours(168);
/// Largest response the cache will store.
const MAX_CACHE_BYTES: usize = 8 * 1024 * 1024;

/// Errors returned while fetching or decoding GDACS responses.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The HTTP request failed.
    Http(reqwest::Error),
    /// GDACS returned a non-success status.
    Status {
        /// Requested URL.
        url: String,
        /// HTTP status code.
        status: u16,
    },
    /// The response was not a GDACS feature collection.
    Decode(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(error) => write!(f, "GDACS request failed: {error}"),
            Self::Status { url, status } => write!(f, "GDACS returned HTTP {status} for {url}"),
            Self::Decode(message) => write!(f, "GDACS response is invalid: {message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            _ => None,
        }
    }
}

/// GDACS alert level; only orange and red are requested by [`Client::alerts`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Level {
    /// Informational.
    Green,
    /// Elevated humanitarian concern.
    Orange,
    /// Highest humanitarian concern.
    Red,
}

impl Level {
    /// Parses the API's `alertlevel` text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "green" => Some(Self::Green),
            "orange" => Some(Self::Orange),
            "red" => Some(Self::Red),
            _ => None,
        }
    }
    /// Lowercase name, as used in record values.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Green => "green",
            Self::Orange => "orange",
            Self::Red => "red",
        }
    }
    fn api_name(self) -> &'static str {
        match self {
            Self::Green => "Green",
            Self::Orange => "Orange",
            Self::Red => "Red",
        }
    }
}

/// One GDACS event episode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    /// Two-letter hazard code: `EQ`, `TC`, `FL`, `VO`, `DR` or `WF`.
    pub event_type: String,
    /// GDACS event id.
    pub event_id: u64,
    /// Alert level of the event.
    pub level: Level,
    /// Event title.
    pub name: String,
    /// First day the alert applies.
    pub from: NaiveDate,
    /// Last day the alert applies.
    pub to: NaiveDate,
    /// Whether GDACS flags the event as current.
    pub current: bool,
    /// ISO 3166-1 alpha-3 codes of every affected country.
    pub iso3: Vec<String>,
    /// Human-readable report page.
    pub report_url: String,
}

/// Where a response came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Fetched from the live API.
    Api,
    /// Served from the TTL-bounded fallback cache.
    Cache,
}

impl Origin {
    /// The `measurement.source` word for this origin.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Cache => "cache",
        }
    }
}

/// Blocking GDACS client with an optional caller-owned fallback cache.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    cache_dir: Option<PathBuf>,
    ttl: Duration,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Creates a client with live access and no cache.
    #[must_use]
    pub fn new() -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent("statsapi-gdacs/0.1")
            .build()
            .unwrap_or_default();
        Self {
            http,
            cache_dir: None,
            ttl: MAX_CACHE_TTL,
        }
    }
    /// Enables a fallback cache; `ttl` is clamped to [`MAX_CACHE_TTL`].
    #[must_use]
    pub fn with_cache(mut self, dir: impl AsRef<Path>, ttl: Duration) -> Self {
        self.cache_dir = Some(dir.as_ref().to_path_buf());
        self.ttl = ttl.min(MAX_CACHE_TTL);
        self
    }
    /// Builds the event-list URL for orange and red alerts over a date range.
    #[must_use]
    pub fn alerts_url(from: NaiveDate, to: NaiveDate) -> String {
        Self::alerts_url_with_levels(from, to, &[Level::Orange, Level::Red])
    }
    /// Builds the event-list URL for the requested alert levels over a date
    /// range.
    #[must_use]
    pub fn alerts_url_with_levels(from: NaiveDate, to: NaiveDate, levels: &[Level]) -> String {
        let levels = if levels.is_empty() {
            vec![Level::Orange, Level::Red]
        } else {
            levels.to_vec()
        };
        let alert_levels = levels
            .iter()
            .map(|level| level.api_name())
            .collect::<Vec<_>>()
            .join(";");
        format!(
            "{BASE}?eventlist=EQ;TC;FL;VO;DR;WF&alertlevel={alert_levels}&fromDate={from}&toDate={to}"
        )
    }
    /// Fetches orange and red alerts overlapping `from..=to`.
    ///
    /// # Errors
    /// Returns an error when the live API and any fresh cache entry both fail,
    /// or the body is not a GDACS feature collection.
    pub fn alerts(&self, from: NaiveDate, to: NaiveDate) -> Result<(Vec<Alert>, Origin), Error> {
        self.alerts_with_levels(from, to, &[Level::Orange, Level::Red])
    }
    /// Fetches alerts for the requested levels, using the same live-first,
    /// TTL-bounded cache policy as [`Client::alerts`].
    ///
    /// # Errors
    /// Returns an error when the live API and any fresh cache entry both fail,
    /// or the body is not a GDACS feature collection.
    pub fn alerts_with_levels(
        &self,
        from: NaiveDate,
        to: NaiveDate,
        levels: &[Level],
    ) -> Result<(Vec<Alert>, Origin), Error> {
        let url = Self::alerts_url_with_levels(from, to, levels);
        let (bytes, origin) = match self.fetch(&url) {
            Ok(bytes) => {
                self.store(&url, &bytes);
                (bytes, Origin::Api)
            }
            Err(error) => (self.cached(&url).ok_or(error)?, Origin::Cache),
        };
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|e| Error::Decode(e.to_string()))?;
        Ok((decode_alerts(&value)?, origin))
    }
    fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        let mut last = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let jitter = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |now| u64::from(now.subsec_millis()) % 250);
                std::thread::sleep(Duration::from_millis(
                    250_u64.saturating_mul(1 << attempt) + jitter,
                ));
            }
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    return response.bytes().map(|b| b.to_vec()).map_err(Error::Http);
                }
                Ok(response) => {
                    let status = response.status();
                    let error = Error::Status {
                        url: url.to_owned(),
                        status: status.as_u16(),
                    };
                    if status.as_u16() != 429 && !status.is_server_error() {
                        return Err(error);
                    }
                    last = Some(error);
                }
                Err(error) => last = Some(Error::Http(error)),
            }
        }
        Err(last.unwrap_or_else(|| Error::Decode("no attempt made".to_owned())))
    }
    fn path(&self, url: &str) -> Option<PathBuf> {
        let dir = self.cache_dir.as_ref()?;
        Some(dir.join(format!("{:x}", Sha256::digest(url.as_bytes()))))
    }
    fn cached(&self, url: &str) -> Option<Vec<u8>> {
        let path = self.path(url)?;
        let age = std::fs::metadata(&path)
            .ok()?
            .modified()
            .ok()?
            .elapsed()
            .ok()?;
        (age <= self.ttl)
            .then(|| std::fs::read(path).ok())
            .flatten()
    }
    /// Best effort: a failed cache write never fails a live read.
    fn store(&self, url: &str, bytes: &[u8]) {
        let Some(path) = self.path(url) else { return };
        if bytes.len() > MAX_CACHE_BYTES {
            return;
        }
        let Some(parent) = path.parent() else { return };
        let temp = path.with_extension(format!("tmp.{}", std::process::id()));
        if std::fs::create_dir_all(parent).is_ok()
            && std::fs::write(&temp, bytes).is_ok()
            && std::fs::rename(&temp, path).is_err()
        {
            let _ = std::fs::remove_file(temp);
        }
    }
}

/// Decodes a GDACS feature collection into alerts, skipping malformed features.
///
/// # Errors
/// Returns an error when `features` is missing.
pub fn decode_alerts(value: &Value) -> Result<Vec<Alert>, Error> {
    let features = value["features"]
        .as_array()
        .ok_or_else(|| Error::Decode("missing features array".to_owned()))?;
    Ok(features.iter().filter_map(decode_feature).collect())
}

fn decode_feature(feature: &Value) -> Option<Alert> {
    let p = &feature["properties"];
    let date = |key: &str| NaiveDate::parse_from_str(p[key].as_str()?.get(..10)?, "%Y-%m-%d").ok();
    let mut iso3: Vec<String> = p["affectedcountries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["iso3"].as_str().map(str::to_owned))
        .collect();
    if iso3.is_empty() {
        iso3.extend(
            p["iso3"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
        );
    }
    Some(Alert {
        event_type: p["eventtype"].as_str()?.to_owned(),
        event_id: p["eventid"].as_u64()?,
        level: Level::parse(p["alertlevel"].as_str()?)?,
        name: p["name"]
            .as_str()
            .or(p["eventname"].as_str())?
            .trim()
            .to_owned(),
        from: date("fromdate")?,
        to: date("todate")?,
        current: p["iscurrent"].as_str() == Some("true") || p["iscurrent"].as_bool() == Some(true),
        iso3,
        report_url: p["url"]["report"].as_str().unwrap_or_default().to_owned(),
    })
}
