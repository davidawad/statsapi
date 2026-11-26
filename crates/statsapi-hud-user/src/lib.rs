//! Reusable blocking client for HUD USER's authenticated public data APIs.
use reqwest::blocking::{Client as HttpClient, Response};
use reqwest::Url;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const FMR_BASE: &str = "https://www.huduser.gov/hudapi/public";
const USPS_BASE: &str = "https://www.huduser.gov/hudapi/public/usps";
const CACHE_PREFIX: &str = "statsapi-hud-user-";
const HEX: &[u8; 16] = b"0123456789abcdef";
const MAX_ATTEMPTS: u32 = 4;

/// An API, transport, cache, or response decoding error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HudUserError {
    message: String,
}

impl HudUserError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for HudUserError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for HudUserError {}

/// A client whose credential and optional disk cache are supplied by its caller.
#[derive(Clone)]
pub struct HudUserClient {
    bearer_token: String,
    http: HttpClient,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl fmt::Debug for HudUserClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HudUserClient")
            .field("bearer_token", &"[REDACTED]")
            .field("cache_dir", &self.cache_dir)
            .finish_non_exhaustive()
    }
}

impl HudUserClient {
    /// Create a client with a caller-provided bearer token and no disk cache.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be configured.
    pub fn new(token: impl Into<String>) -> Result<Self, HudUserError> {
        Self::builder(token).build()
    }

    /// Start configuring a client with an optional caller-owned cache directory.
    #[must_use]
    pub fn builder(token: impl Into<String>) -> HudUserClientBuilder {
        HudUserClientBuilder {
            token: token.into(),
            cache_dir: None,
            offline: false,
        }
    }

    /// Fetch a Fair Market Rent response for a HUD entity id and optional year.
    ///
    /// # Errors
    /// Returns an API, transport, cache, or JSON decoding error.
    pub fn fmr(&self, entity_id: &str, year: Option<u16>) -> Result<Value, HudUserError> {
        self.get(
            "fmr",
            "data",
            entity_id,
            year.map(|year| year.to_string()).as_deref(),
        )
    }

    /// Fetch an Income Limits response for a HUD entity id and optional year.
    ///
    /// # Errors
    /// Returns an API, transport, cache, or JSON decoding error.
    pub fn income_limits(&self, entity_id: &str, year: Option<u16>) -> Result<Value, HudUserError> {
        self.get(
            "il",
            "data",
            entity_id,
            year.map(|year| year.to_string()).as_deref(),
        )
    }

    /// Fetch USPS crosswalk rows for a ZIP, crosswalk type, and optional period.
    ///
    /// Types 1 and 2 are ZIP-to-tract and ZIP-to-county. The API also accepts
    /// types 3 through 12; the raw response rows are returned unchanged.
    ///
    /// # Errors
    /// Returns an input, API, transport, cache, or JSON decoding error.
    pub fn zip_crosswalk(
        &self,
        zip: &str,
        crosswalk_type: u8,
        year: Option<u16>,
        quarter: Option<u8>,
    ) -> Result<Value, HudUserError> {
        if zip.len() != 5 || !zip.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(HudUserError::new("ZIP must contain exactly five digits"));
        }
        if !(1..=12).contains(&crosswalk_type) {
            return Err(HudUserError::new("crosswalk type must be between 1 and 12"));
        }
        if quarter.is_some_and(|value| !(1..=4).contains(&value)) {
            return Err(HudUserError::new("quarter must be between 1 and 4"));
        }
        let mut url =
            Url::parse(USPS_BASE).map_err(|error| HudUserError::new(error.to_string()))?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("type", &crosswalk_type.to_string())
                .append_pair("query", zip);
            if let Some(year) = year {
                query.append_pair("year", &year.to_string());
            }
            if let Some(quarter) = quarter {
                query.append_pair("quarter", &quarter.to_string());
            }
        }
        self.get_url(&url)
    }

    /// Fetch and decode a JSON API response from a HUD endpoint path.
    ///
    /// This lower-level method is useful for endpoints not wrapped by a helper.
    /// Paths are relative to the HUD API base (`https://www.huduser.gov/hudapi/public`) and must not begin with `/`.
    ///
    /// # Errors
    /// Returns an API, transport, cache, or JSON decoding error.
    pub fn get_fmr_json(&self, path: &str, year: Option<u16>) -> Result<Value, HudUserError> {
        if path.starts_with('/') || path.contains("..") {
            return Err(HudUserError::new("HUD endpoint path must be relative"));
        }
        let mut url = Url::parse(&format!("{FMR_BASE}/{path}"))
            .map_err(|error| HudUserError::new(error.to_string()))?;
        if let Some(year) = year {
            url.query_pairs_mut().append_pair("year", &year.to_string());
        }
        self.get_url(&url)
    }

    fn get(
        &self,
        family: &str,
        endpoint: &str,
        entity_id: &str,
        year: Option<&str>,
    ) -> Result<Value, HudUserError> {
        if entity_id.is_empty() || entity_id.contains('/') || entity_id.contains('?') {
            return Err(HudUserError::new(
                "HUD entity id must be a non-empty path segment",
            ));
        }
        let mut url = Url::parse(&format!("{FMR_BASE}/{family}/{endpoint}/{entity_id}"))
            .map_err(|error| HudUserError::new(error.to_string()))?;
        if let Some(year) = year {
            url.query_pairs_mut().append_pair("year", year);
        }
        self.get_url(&url)
    }

    fn get_url(&self, url: &Url) -> Result<Value, HudUserError> {
        let url_text = url.as_str();
        if let Some(dir) = &self.cache_dir {
            let path = dir.join(format!(
                "{}{}",
                CACHE_PREFIX,
                hex(&Sha256::digest(url_text.as_bytes()))
            ));
            if let Ok(bytes) = std::fs::read(&path) {
                return decode_json(&bytes);
            }
            if self.offline {
                return Err(HudUserError::new("offline cache miss"));
            }
            let bytes = self.fetch(url_text)?;
            std::fs::create_dir_all(dir).map_err(|error| HudUserError::new(error.to_string()))?;
            let temp = dir.join(format!(
                ".{CACHE_PREFIX}{}.{}.tmp",
                hex(&Sha256::digest(url_text.as_bytes())),
                std::process::id()
            ));
            std::fs::write(&temp, &bytes).map_err(|error| HudUserError::new(error.to_string()))?;
            std::fs::rename(temp, path).map_err(|error| HudUserError::new(error.to_string()))?;
            decode_json(&bytes)
        } else if self.offline {
            Err(HudUserError::new(
                "offline mode requires a caller-supplied cache directory",
            ))
        } else {
            decode_json(&self.fetch(url_text)?)
        }
    }

    fn fetch(&self, url: &str) -> Result<Vec<u8>, HudUserError> {
        for attempt in 0..MAX_ATTEMPTS {
            match self.http.get(url).bearer_auth(&self.bearer_token).send() {
                Ok(response)
                    if retryable(response.status().as_u16()) && attempt + 1 < MAX_ATTEMPTS =>
                {
                    thread::sleep(retry_delay(attempt));
                }
                Ok(response) => return response_bytes(response),
                Err(error) if attempt + 1 < MAX_ATTEMPTS => {
                    let _ = error;
                    thread::sleep(retry_delay(attempt));
                }
                Err(error) => return Err(HudUserError::new(error.to_string())),
            }
        }
        Err(HudUserError::new("HUD API retries exhausted"))
    }
}

/// Builder for [`HudUserClient`].
pub struct HudUserClientBuilder {
    token: String,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl HudUserClientBuilder {
    /// Set a caller-supplied directory for URL-keyed response caching.
    #[must_use]
    pub fn cache_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Restrict requests to cache hits; network access is disabled for this client.
    #[must_use]
    pub fn offline(mut self) -> Self {
        self.offline = true;
        self
    }

    /// Build the blocking HTTP client.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be configured.
    pub fn build(self) -> Result<HudUserClient, HudUserError> {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(45))
            .build()
            .map_err(|error| HudUserError::new(error.to_string()))?;
        Ok(HudUserClient {
            bearer_token: self.token,
            http,
            cache_dir: self.cache_dir,
            offline: self.offline,
        })
    }
}

/// Decode a HUD JSON response body.
///
/// # Errors
/// Returns an error when the response is not valid JSON.
pub fn decode_json(bytes: &[u8]) -> Result<Value, HudUserError> {
    serde_json::from_slice(bytes).map_err(|error| HudUserError::new(format!("HUD JSON: {error}")))
}

fn response_bytes(response: Response) -> Result<Vec<u8>, HudUserError> {
    response
        .error_for_status()
        .map_err(|error| HudUserError::new(error.to_string()))?
        .bytes()
        .map(|bytes| bytes.to_vec())
        .map_err(|error| HudUserError::new(error.to_string()))
}

fn retryable(status: u16) -> bool {
    status == 429 || (500..=599).contains(&status)
}

fn retry_delay(attempt: u32) -> Duration {
    let base_ms = 100_u64.saturating_mul(1_u64 << attempt.min(5));
    let jitter_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| u64::from(time.subsec_nanos()) % 100);
    Duration::from_millis(base_ms + jitter_ms)
}

fn hex(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_fmr_fixture_retains_upstream_layout() {
        let payload =
            decode_json(include_bytes!("../tests/fixtures/fmr.json")).expect("fixture JSON");
        assert_eq!(
            payload
                .pointer("/data/basicdata/Two-Bedroom")
                .and_then(Value::as_str),
            Some("1375")
        );
        assert_eq!(
            payload.pointer("/data/year").and_then(Value::as_str),
            Some("2098")
        );
    }

    #[test]
    fn synthetic_small_area_fixture_retains_upstream_layout() {
        let payload =
            decode_json(include_bytes!("../tests/fixtures/safmr.json")).expect("fixture JSON");
        assert_eq!(
            payload
                .pointer("/data/basicdata/1/zip_code")
                .and_then(Value::as_str),
            Some("00001")
        );
        assert_eq!(
            payload
                .pointer("/data/basicdata/1/Two-Bedroom")
                .and_then(Value::as_u64),
            Some(2100)
        );
    }

    #[test]
    fn synthetic_income_and_crosswalk_fixtures_decode() {
        let income = decode_json(include_bytes!("../tests/fixtures/income-limits.json"))
            .expect("income fixture");
        let crosswalk = decode_json(include_bytes!("../tests/fixtures/zip-crosswalk.json"))
            .expect("crosswalk fixture");
        assert_eq!(
            income
                .pointer("/data/median_income")
                .and_then(Value::as_u64),
            Some(77777)
        );
        assert_eq!(
            crosswalk
                .pointer("/data/results/0/res_ratio")
                .and_then(Value::as_f64),
            Some(0.41)
        );
    }

    #[test]
    fn offline_mode_without_cache_fails_without_network() {
        let client = HudUserClient::builder("test-token")
            .offline()
            .build()
            .expect("client");
        assert_eq!(
            client
                .fmr("3402399999", None)
                .expect_err("no cache configured")
                .to_string(),
            "offline mode requires a caller-supplied cache directory"
        );
    }

    #[test]
    fn validates_crosswalk_arguments_without_network_or_credentials() {
        let client = HudUserClient::new("test-token").expect("client");
        assert!(client.zip_crosswalk("1234", 1, None, None).is_err());
        assert!(client.zip_crosswalk("12345", 13, None, None).is_err());
        assert!(client.zip_crosswalk("12345", 1, None, Some(5)).is_err());
    }

    #[test]
    #[ignore = "live HUD USER API check requires a caller-provided HUD_API_TOKEN"]
    fn live_api_smoke() {
        let Some(token) = std::env::var("HUD_API_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty())
        else {
            println!("credential_missing: HUD_API_TOKEN");
            return;
        };
        let client = HudUserClient::builder(token).build().expect("client");
        let response = client.fmr("3402399999", None).expect("FMR response");
        assert!(response.get("data").is_some());
        println!("LIVE HUD FMR endpoint=fmr/data/3402399999 date=2026-11-22");
    }
}
