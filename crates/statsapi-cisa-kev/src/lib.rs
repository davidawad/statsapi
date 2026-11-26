//! Blocking client for the CISA Known Exploited Vulnerabilities (KEV) catalog
//! and the NVD CVE 2.0 API, with no repository dependencies.
//!
//! The catalog is streamed through the JSON decoder and reduced to the fields
//! the summaries need; nothing is written to disk.

use chrono::{Duration as Days, NaiveDate, NaiveDateTime};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::Read;
use std::time::Duration;
use thiserror::Error;

/// CISA's published catalog feed.
pub const CISA_KEV_URL: &str =
    "https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json";
/// CISA's own GitHub mirror of the same feed, used when cisa.gov refuses the request.
pub const CISA_KEV_MIRROR_URL: &str =
    "https://raw.githubusercontent.com/cisagov/kev-data/develop/known_exploited_vulnerabilities.json";
/// NVD CVE 2.0 endpoint.
pub const NVD_CVE_URL: &str = "https://services.nvd.nist.gov/rest/json/cves/2.0";

/// Largest catalog body the decoder will read (the feed is about 2 MiB).
const MAX_BODY: u64 = 32 * 1024 * 1024;
const ATTEMPTS: u32 = 4;
const USER_AGENT: &str = concat!("statsapi-cisa-kev/", env!("CARGO_PKG_VERSION"));
/// `knownRansomwareCampaignUse` value for entries tied to ransomware.
const RANSOMWARE_KNOWN: &str = "Known";

/// Errors returned by the KEV and NVD clients.
#[derive(Debug, Error)]
pub enum Error {
    /// The request failed at the transport layer.
    #[error("request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// The server answered with a non-success status after any retries.
    #[error("{url} answered HTTP {status}")]
    Status {
        /// Requested URL without credentials.
        url: String,
        /// Final HTTP status code.
        status: u16,
    },
    /// The body was not the expected JSON, or exceeded the size cap.
    #[error("invalid response: {0}")]
    Decode(String),
    /// A date in the catalog did not parse.
    #[error("invalid date {0:?}")]
    Date(String),
}

/// One catalog entry, reduced to the fields used for summaries.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// CVE identifier.
    #[serde(rename = "cveID")]
    pub cve_id: String,
    /// Vendor or project name as published.
    pub vendor_project: String,
    /// Date CISA added the entry (`YYYY-MM-DD`).
    pub date_added: String,
    /// `Known` or `Unknown`.
    #[serde(default)]
    pub known_ransomware_campaign_use: String,
}

/// The decoded catalog and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    /// Catalog release label, such as `2026.11.20`.
    #[serde(default)]
    pub catalog_version: String,
    /// Release timestamp as published.
    #[serde(default)]
    pub date_released: String,
    /// Entries in the catalog.
    pub vulnerabilities: Vec<Entry>,
    /// URL the catalog was read from; empty for [`Catalog::from_reader`].
    #[serde(skip)]
    pub source_url: String,
}

/// Count of additions by one vendor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VendorCount {
    /// Vendor or project name.
    pub vendor: String,
    /// Entries added in the window.
    pub added: u64,
}

/// Window and catalog-wide figures derived from a [`Catalog`].
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    /// Total entries in the catalog.
    pub catalog_size: u64,
    /// Entries added in the last 7 days (inclusive of `today`).
    pub added_7d: u64,
    /// Entries added in the last 30 days.
    pub added_30d: u64,
    /// Entries added in the last 90 days.
    pub added_90d: u64,
    /// Percent of the whole catalog marked ransomware-associated.
    pub ransomware_share: f64,
    /// Percent of the last 90 days' additions marked ransomware-associated.
    pub ransomware_share_90d: f64,
    /// Vendors by additions in the last 90 days, most first then by name.
    pub top_vendors_90d: Vec<VendorCount>,
}

impl Catalog {
    /// Decodes a catalog from any reader, refusing bodies over 32 MiB.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Decode`] for malformed JSON or an oversized body.
    pub fn from_reader(reader: impl Read) -> Result<Self, Error> {
        let mut limited = reader.take(MAX_BODY + 1);
        let catalog: Self = serde_json::from_reader(&mut limited)
            .map_err(|error| Error::Decode(error.to_string()))?;
        if limited.limit() == 0 {
            return Err(Error::Decode("catalog exceeds the 32 MiB cap".into()));
        }
        Ok(catalog)
    }

    /// Computes window counts, ransomware shares and top vendors as of `today`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Date`] when an entry's `dateAdded` is not `YYYY-MM-DD`.
    pub fn summarize(&self, today: NaiveDate) -> Result<Summary, Error> {
        let mut summary = Summary {
            catalog_size: self.vulnerabilities.len() as u64,
            added_7d: 0,
            added_30d: 0,
            added_90d: 0,
            ransomware_share: 0.0,
            ransomware_share_90d: 0.0,
            top_vendors_90d: Vec::new(),
        };
        let mut ransomware_all = 0_u64;
        let mut ransomware_90d = 0_u64;
        let mut vendors: BTreeMap<&str, u64> = BTreeMap::new();
        for entry in &self.vulnerabilities {
            let added = NaiveDate::parse_from_str(&entry.date_added, "%Y-%m-%d")
                .map_err(|_| Error::Date(entry.date_added.clone()))?;
            let age = (today - added).num_days();
            let ransomware = entry.known_ransomware_campaign_use == RANSOMWARE_KNOWN;
            ransomware_all += u64::from(ransomware);
            if !(0..90).contains(&age) {
                continue;
            }
            summary.added_90d += 1;
            ransomware_90d += u64::from(ransomware);
            *vendors.entry(&entry.vendor_project).or_default() += 1;
            if age < 30 {
                summary.added_30d += 1;
            }
            if age < 7 {
                summary.added_7d += 1;
            }
        }
        summary.ransomware_share = percent(ransomware_all, summary.catalog_size);
        summary.ransomware_share_90d = percent(ransomware_90d, summary.added_90d);
        let mut ranked: Vec<VendorCount> = vendors
            .into_iter()
            .map(|(vendor, added)| VendorCount {
                vendor: vendor.to_owned(),
                added,
            })
            .collect();
        ranked.sort_by(|a, b| b.added.cmp(&a.added).then_with(|| a.vendor.cmp(&b.vendor)));
        summary.top_vendors_90d = ranked;
        Ok(summary)
    }
}

#[expect(clippy::cast_precision_loss, reason = "counts stay far below 2^52")]
fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        (part as f64 / whole as f64 * 10_000.0).round() / 100.0
    }
}

/// Blocking client for the KEV feed and NVD CVE counts.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    kev_urls: Vec<String>,
    nvd_url: String,
    retry_pause: Duration,
}

impl Client {
    /// Builds a client with the published CISA and NVD endpoints.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the HTTP client cannot be constructed.
    pub fn new() -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self {
            http,
            kev_urls: vec![CISA_KEV_URL.into(), CISA_KEV_MIRROR_URL.into()],
            nvd_url: NVD_CVE_URL.into(),
            retry_pause: Duration::from_secs(2),
        })
    }

    /// Replaces the catalog URLs, tried in order.
    #[must_use]
    pub fn with_kev_urls(mut self, urls: Vec<String>) -> Self {
        self.kev_urls = urls;
        self
    }

    /// Replaces the NVD CVE endpoint.
    #[must_use]
    pub fn with_nvd_url(mut self, url: impl Into<String>) -> Self {
        self.nvd_url = url.into();
        self
    }

    /// Sets the base pause between retries (jitter is added).
    #[must_use]
    pub fn with_retry_pause(mut self, pause: Duration) -> Self {
        self.retry_pause = pause;
        self
    }

    /// Streams the catalog from the first URL that answers, persisting nothing.
    ///
    /// # Errors
    ///
    /// Returns the last URL's error when every catalog URL fails.
    pub fn fetch_catalog(&self) -> Result<Catalog, Error> {
        let mut last = Error::Decode("no catalog URL configured".into());
        for url in &self.kev_urls {
            match self.get(url).and_then(Catalog::from_reader) {
                Ok(mut catalog) => {
                    catalog.source_url.clone_from(url);
                    return Ok(catalog);
                }
                Err(error) => last = error,
            }
        }
        Err(last)
    }

    /// Counts CVEs rated CRITICAL (CVSS v3) published in the `days` before `now`.
    ///
    /// The NVD window limit is 120 days; larger values are rejected.
    ///
    /// # Errors
    ///
    /// Returns an error for an out-of-range window, a failed request, or a
    /// response without `totalResults`.
    pub fn critical_cves_published(
        &self,
        now: NaiveDateTime,
        days: i64,
    ) -> Result<(u64, String), Error> {
        if !(1..=120).contains(&days) {
            return Err(Error::Decode(format!(
                "NVD window {days} days is not 1..=120"
            )));
        }
        let fmt = "%Y-%m-%dT%H:%M:%S%.3f";
        let url = format!(
            "{}?cvssV3Severity=CRITICAL&resultsPerPage=1&pubStartDate={}&pubEndDate={}",
            self.nvd_url,
            (now - Days::days(days)).format(fmt),
            now.format(fmt)
        );
        let body: serde_json::Value = serde_json::from_reader(self.get(&url)?.take(MAX_BODY))
            .map_err(|error| Error::Decode(error.to_string()))?;
        let total = body
            .get("totalResults")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| Error::Decode("NVD response lacks totalResults".into()))?;
        Ok((total, url))
    }

    /// GET with jittered retry on 429, 5xx and transport errors.
    fn get(&self, url: &str) -> Result<reqwest::blocking::Response, Error> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let result = self.http.get(url).send();
            let retryable = match &result {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return result.map_err(Error::from);
                    }
                    if !(status.as_u16() == 429 || status.is_server_error()) || attempt >= ATTEMPTS
                    {
                        return Err(Error::Status {
                            url: url.split('?').next().unwrap_or(url).to_owned(),
                            status: status.as_u16(),
                        });
                    }
                    true
                }
                Err(_) => attempt < ATTEMPTS,
            };
            if !retryable {
                return result.map_err(Error::from);
            }
            std::thread::sleep(self.retry_pause * attempt + jitter());
        }
    }
}

fn jitter() -> Duration {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    Duration::from_millis(u64::from(nanos % 500))
}
