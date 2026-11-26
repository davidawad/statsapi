//! Blocking client for the `ReliefWeb` API v2 (disasters and reports).
//!
//! Source order: the live API first; an optional, TTL-bounded, size-capped
//! cache directory is read only when the live request fails.

use chrono::NaiveDate;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BASE: &str = "https://api.reliefweb.int/v2";
/// Appname registered with `ReliefWeb` for this repo; not a secret.
/// Environment variable holding your approved `ReliefWeb` appname.
pub const APPNAME_ENV: &str = "RELIEFWEB_APPNAME";
const MAX_RETRIES: u32 = 3;
/// Rows requested per disaster query (the API allows up to 1000).
const DISASTER_LIMIT: u32 = 1000;
/// Longest a cached response may be served; the repo rule is seven days.
pub const MAX_CACHE_TTL: Duration = Duration::from_hours(168);
/// Largest response the cache will store.
const MAX_CACHE_BYTES: usize = 8 * 1024 * 1024;

/// Errors returned while fetching or decoding `ReliefWeb` responses.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The HTTP request failed.
    Http(reqwest::Error),
    /// `ReliefWeb` returned a non-success status.
    Status {
        /// Requested endpoint (without the appname).
        endpoint: String,
        /// HTTP status code.
        status: u16,
    },
    /// The response was not a `ReliefWeb` result list.
    Decode(String),
    /// No appname: `ReliefWeb` v2 requires an approved one (`RELIEFWEB_APPNAME`
    /// or [`Client::with_appname`]); request yours at
    /// <https://apidoc.reliefweb.int/parameters#appname>.
    MissingAppname,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(error) => write!(f, "ReliefWeb request failed: {error}"),
            Self::Status { endpoint, status } => {
                write!(f, "ReliefWeb returned HTTP {status} for {endpoint}")
            }
            Self::Decode(message) => write!(f, "ReliefWeb response is invalid: {message}"),
            Self::MissingAppname => write!(
                f,
                "ReliefWeb appname is not set: set RELIEFWEB_APPNAME or use Client::with_appname"
            ),
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

/// `ReliefWeb` disaster status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Status {
    /// Alert: a disaster that may develop.
    Alert,
    /// Ongoing disaster.
    Ongoing,
    /// Past disaster.
    Past,
}

impl Status {
    /// The API's lowercase status word.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Alert => "alert",
            Self::Ongoing => "ongoing",
            Self::Past => "past",
        }
    }
    /// Parses the API's status word.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "alert" => Some(Self::Alert),
            "ongoing" => Some(Self::Ongoing),
            "past" => Some(Self::Past),
            _ => None,
        }
    }
}

/// One `ReliefWeb` disaster.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disaster {
    /// `ReliefWeb` disaster id.
    pub id: u64,
    /// Disaster title, for example `Haiti: Floods - Oct 2026`.
    pub name: String,
    /// Disaster status.
    pub status: Status,
    /// Event date.
    pub date: NaiveDate,
    /// Primary disaster type name, for example `Flood`.
    pub primary_type: String,
    /// Public `ReliefWeb` page.
    pub url: String,
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

/// Which disasters to list for a country.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisasterFilter {
    /// Keep only these statuses; empty keeps all.
    pub statuses: Vec<Status>,
    /// Keep only disasters whose event date is on or after this day.
    pub since: Option<NaiveDate>,
    /// Maximum rows, newest event first; `None` asks for the API maximum.
    pub limit: Option<u32>,
}

/// Blocking `ReliefWeb` client with an optional caller-owned fallback cache.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    appname: String,
    cache_dir: Option<PathBuf>,
    ttl: Duration,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Creates a client using the `RELIEFWEB_APPNAME` environment variable, with
    /// no cache. Requests fail with [`Error::MissingAppname`] when it is unset.
    #[must_use]
    pub fn new() -> Self {
        let appname = std::env::var(APPNAME_ENV)
            .ok()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_default();
        Self::with_appname(appname)
    }
    /// Creates a client that identifies itself to `ReliefWeb` as `appname`.
    #[must_use]
    pub fn with_appname(appname: impl Into<String>) -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent("statsapi-reliefweb/0.1")
            .build()
            .unwrap_or_default();
        Self {
            http,
            appname: appname.into(),
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
    /// Public endpoint URL (no appname) for `disasters` or `reports`.
    #[must_use]
    pub fn endpoint(name: &str) -> String {
        format!("{BASE}/{name}")
    }
    /// Request body listing a country's disasters, newest event first.
    #[must_use]
    pub fn disasters_body(iso3: &str, filter: &DisasterFilter) -> Value {
        let mut conditions = vec![country_condition(iso3)];
        if !filter.statuses.is_empty() {
            let words: Vec<&str> = filter.statuses.iter().map(|s| s.as_str()).collect();
            conditions.push(json!({"field":"status","value":words,"operator":"OR"}));
        }
        if let Some(since) = filter.since {
            conditions.push(
                json!({"field":"date.event","value":{"from":format!("{since}T00:00:00+00:00")}}),
            );
        }
        json!({"limit":filter.limit.unwrap_or(DISASTER_LIMIT).min(DISASTER_LIMIT),
            "fields":{"include":["name","status","date.event","primary_type.name","url_alias"]},
            "sort":["date.event:desc"],
            "filter":{"operator":"AND","conditions":conditions}})
    }
    /// Request body counting a country's reports created on or after `since`.
    #[must_use]
    pub fn reports_body(iso3: &str, since: NaiveDate) -> Value {
        json!({"limit":0,
            "filter":{"operator":"AND","conditions":[country_condition(iso3),
                {"field":"date.created","value":{"from":format!("{since}T00:00:00+00:00")}}]}})
    }
    /// Lists a country's disasters, newest event first.
    ///
    /// # Errors
    /// Returns an error when the live API and any fresh cache entry both fail,
    /// or the body is not a `ReliefWeb` result list.
    pub fn disasters(
        &self,
        iso3: &str,
        filter: &DisasterFilter,
    ) -> Result<(Vec<Disaster>, Origin), Error> {
        let (value, origin) = self.post("disasters", &Self::disasters_body(iso3, filter))?;
        Ok((decode_disasters(&value)?, origin))
    }
    /// Counts reports about a country created on or after `since`.
    ///
    /// # Errors
    /// Returns an error when the live API and any fresh cache entry both fail,
    /// or the body carries no `totalCount`.
    pub fn report_count(&self, iso3: &str, since: NaiveDate) -> Result<(u64, Origin), Error> {
        let (value, origin) = self.post("reports", &Self::reports_body(iso3, since))?;
        let total = value["totalCount"]
            .as_u64()
            .ok_or_else(|| Error::Decode("missing totalCount".to_owned()))?;
        Ok((total, origin))
    }
    fn post(&self, endpoint: &str, body: &Value) -> Result<(Value, Origin), Error> {
        let key = format!("{endpoint} {body}");
        let (bytes, origin) = match self.fetch(endpoint, body) {
            Ok(bytes) => {
                self.store(&key, &bytes);
                (bytes, Origin::Api)
            }
            Err(error) => (self.cached(&key).ok_or(error)?, Origin::Cache),
        };
        let value = serde_json::from_slice(&bytes).map_err(|e| Error::Decode(e.to_string()))?;
        Ok((value, origin))
    }
    fn fetch(&self, endpoint: &str, body: &Value) -> Result<Vec<u8>, Error> {
        let url = Self::endpoint(endpoint);
        let mut last = None;
        if self.appname.trim().is_empty() {
            return Err(Error::MissingAppname);
        }
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let jitter = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |now| u64::from(now.subsec_millis()) % 250);
                std::thread::sleep(Duration::from_millis(
                    250_u64.saturating_mul(1 << attempt) + jitter,
                ));
            }
            let sent = self
                .http
                .post(&url)
                .query(&[("appname", self.appname.as_str())])
                .json(body)
                .send();
            match sent {
                Ok(response) if response.status().is_success() => {
                    return response.bytes().map(|b| b.to_vec()).map_err(Error::Http);
                }
                Ok(response) => {
                    let status = response.status();
                    let error = Error::Status {
                        endpoint: url.clone(),
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
    fn path(&self, key: &str) -> Option<PathBuf> {
        let dir = self.cache_dir.as_ref()?;
        Some(dir.join(format!("{:x}", Sha256::digest(key.as_bytes()))))
    }
    fn cached(&self, key: &str) -> Option<Vec<u8>> {
        let path = self.path(key)?;
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
    fn store(&self, key: &str, bytes: &[u8]) {
        let Some(path) = self.path(key) else { return };
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

fn country_condition(iso3: &str) -> Value {
    json!({"field":"country.iso3","value":iso3.to_ascii_lowercase()})
}

/// Decodes a `ReliefWeb` disaster list, skipping malformed rows.
///
/// # Errors
/// Returns an error when `data` is missing.
pub fn decode_disasters(value: &Value) -> Result<Vec<Disaster>, Error> {
    let rows = value["data"]
        .as_array()
        .ok_or_else(|| Error::Decode("missing data array".to_owned()))?;
    Ok(rows.iter().filter_map(decode_row).collect())
}

fn decode_row(row: &Value) -> Option<Disaster> {
    let f = &row["fields"];
    let id = row["id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| row["id"].as_u64())?;
    Some(Disaster {
        id,
        name: f["name"].as_str()?.trim().to_owned(),
        status: Status::parse(f["status"].as_str()?)?,
        date: NaiveDate::parse_from_str(f["date"]["event"].as_str()?.get(..10)?, "%Y-%m-%d")
            .ok()?,
        primary_type: f["primary_type"]["name"]
            .as_str()
            .unwrap_or("Other")
            .to_owned(),
        url: f["url_alias"].as_str().unwrap_or_default().to_owned(),
    })
}
