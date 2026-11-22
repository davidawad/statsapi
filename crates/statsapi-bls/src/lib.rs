//! A reusable client for BLS labor data APIs and bulk CSV files.

use reqwest::blocking::{Client as HttpClient, RequestBuilder};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

const LAUS_URL: &str = "https://api.bls.gov/publicAPI/v2/timeseries/data/";
const RETRY_LIMIT: usize = 3;

/// A BLS client error that can be displayed and used as an error source.
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

/// A decoded numeric BLS observation and its source metadata.
pub type Observation = (f64, String, String);

/// The blocking BLS API client. Credentials and caching are caller-controlled.
#[derive(Clone)]
pub struct Client {
    http: HttpClient,
    api_key: Option<String>,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl fmt::Debug for Client {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Client")
            .field("api_key", &self.api_key.as_ref().map(|_| "[redacted]"))
            .field("cache_dir", &self.cache_dir)
            .field("offline", &self.offline)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Creates a client with an optional API key and caller-owned cache directory.
    ///
    /// Set `BLS_DATA_OFFLINE=1` to use cache entries only. No global cache path is selected.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn new(api_key: Option<String>, cache_dir: Option<PathBuf>) -> Result<Self, Error> {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(90))
            .build()
            .map_err(|error| Error(error.to_string()))?;
        Ok(Self {
            http,
            api_key: api_key.filter(|key| !key.trim().is_empty()),
            cache_dir,
            offline: std::env::var("BLS_DATA_OFFLINE").as_deref() == Ok("1"),
        })
    }

    /// Creates a client using `BLS_API_KEY` when set and no persistent cache.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn from_env() -> Result<Self, Error> {
        Self::new(std::env::var("BLS_API_KEY").ok(), None)
    }

    /// Fetches and decodes monthly values for one batch of LAUS series identifiers.
    ///
    /// # Errors
    /// Returns an error for transport, HTTP, JSON, or top-level BLS API failures.
    pub fn laus_values(
        &self,
        series: &[String],
    ) -> Result<HashMap<String, Result<Observation, Error>>, Error> {
        if series.is_empty() {
            return Ok(HashMap::new());
        }
        let mut payload = json!({"seriesid": series});
        if let Some(key) = &self.api_key {
            payload["registrationKey"] = Value::String(key.clone());
        }
        let (bytes, _) = self.request(LAUS_URL, Some(&payload))?;
        decode_laus(&bytes)
    }

    /// Fetches the BLS QCEW area CSV and extracts average weekly wage.
    ///
    /// # Errors
    /// Returns an error for transport, HTTP, or malformed CSV responses.
    pub fn qcew_value(&self, county: &str) -> Result<Observation, Error> {
        let url = format!("https://data.bls.gov/cew/data/api/2025/4/area/{county}.csv");
        let (bytes, final_url) = self.request(&url, None)?;
        decode_qcew(&bytes, county, final_url)
    }

    /// Fetches and decodes monthly CES employment for a state or metro area.
    ///
    /// # Errors
    /// Returns an error for transport, HTTP, JSON, or missing observations.
    pub fn ces_value(
        &self,
        level: &str,
        geo_id: &str,
        state_id: &str,
    ) -> Result<Observation, Error> {
        let series = ces_series_id(level, geo_id, state_id)?;
        let mut payload = json!({"seriesid":[series]});
        if let Some(key) = &self.api_key {
            payload["registrationKey"] = Value::String(key.clone());
        }
        let (bytes, _) = self.request(LAUS_URL, Some(&payload))?;
        decode_ces(&bytes, &series)
    }

    fn request(&self, url: &str, body: Option<&Value>) -> Result<(Vec<u8>, String), Error> {
        let identity = body.map_or_else(|| url.to_owned(), |value| format!("{url}:{value}"));
        let path = self.cache_dir.as_ref().map(|directory| {
            directory.join(format!(
                "{}.body",
                hex(&Sha256::digest(identity.as_bytes()))
            ))
        });
        if let Some(path) = &path {
            if let Ok(bytes) = std::fs::read(path) {
                return Ok((bytes, url.to_owned()));
            }
        }
        if self.offline {
            return Err(Error("offline cache miss".into()));
        }
        let mut delay = Duration::from_millis(250);
        for attempt in 0..=RETRY_LIMIT {
            let request = self.request_builder(url, body);
            match request.send() {
                Ok(response) if response.status().is_success() => {
                    let source_url = response.url().to_string();
                    let bytes = response
                        .bytes()
                        .map_err(|error| Error(error.to_string()))?
                        .to_vec();
                    if let Some(path) = path {
                        if let Some(parent) = path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
                        if std::fs::write(&temporary, &bytes).is_ok() {
                            let _ = std::fs::rename(temporary, path);
                        }
                    }
                    return Ok((bytes, source_url));
                }
                Ok(response) => {
                    let status = response.status();
                    if !(status.as_u16() == 429 || status.is_server_error())
                        || attempt == RETRY_LIMIT
                    {
                        return Err(Error(format!("BLS HTTP status {status}")));
                    }
                }
                Err(error) if attempt == RETRY_LIMIT => return Err(Error(error.to_string())),
                Err(_) => {}
            }
            // Bounded jitter spreads retries while respecting the upstream's access limits.
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
        Err(Error("BLS request retry limit reached".into()))
    }

    fn request_builder(&self, url: &str, body: Option<&Value>) -> RequestBuilder {
        match body {
            Some(payload) => self.http.post(url).json(payload),
            None => self.http.get(url),
        }
    }
}

/// Builds a LAUS series identifier for a supported geography and metric suffix.
#[must_use]
pub fn laus_series_id(level: &str, geo_id: &str, metric: &str) -> Option<String> {
    if level == "us" {
        let suffix = match metric {
            "03" => "14000000",
            "04" => "13000000",
            "05" => "12000000",
            "06" => "11000000",
            _ => return None,
        };
        return Some(format!("LNS{suffix}"));
    }
    let prefix = match level {
        "county" => "LAUCN",
        "cbsa" => "LAUMT",
        "state" => "LASST",
        _ => return None,
    };
    Some(format!("{prefix}{geo_id}00000000{metric}"))
}

/// Builds the CES series identifier for state or metro nonfarm employment.
///
/// # Errors
/// Returns an error when the requested geography is unsupported.
pub fn ces_series_id(level: &str, geo_id: &str, state_id: &str) -> Result<String, Error> {
    let area = match level {
        "state" => "00000",
        "cbsa" => geo_id,
        _ => return Err(Error("CES serves CBSA and state".into())),
    };
    Ok(format!("SMU{state_id}{area}0000000001"))
}

/// Decodes one BLS LAUS API response, retaining per-series failures.
///
/// # Errors
/// Returns an error when the top-level response is invalid or unsuccessful.
pub fn decode_laus(bytes: &[u8]) -> Result<HashMap<String, Result<Observation, Error>>, Error> {
    let document: Value =
        serde_json::from_slice(bytes).map_err(|error| Error(format!("BLS JSON: {error}")))?;
    if document.get("status").and_then(Value::as_str) != Some("REQUEST_SUCCEEDED") {
        let message = document
            .get("message")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(Value::as_str)
            .unwrap_or("no details");
        return Err(Error(format!(
            "BLS API status {}: {message}",
            document
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        )));
    }
    let series = document
        .pointer("/Results/series")
        .and_then(Value::as_array)
        .ok_or_else(|| Error("BLS response missing series data".into()))?;
    let mut values = HashMap::new();
    for item in series {
        let Some(id) = item.get("seriesID").and_then(Value::as_str) else {
            continue;
        };
        let decoded = (|| {
            let data = item
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(|| Error("BLS series data missing".into()))?;
            let point = data
                .iter()
                .find(|point| {
                    point
                        .get("period")
                        .and_then(Value::as_str)
                        .is_some_and(|period| period.starts_with('M') && period != "M13")
                })
                .ok_or_else(|| Error("BLS series has no monthly observation".into()))?;
            let number = point
                .get("value")
                .and_then(Value::as_str)
                .ok_or_else(|| Error("BLS value is not a string".into()))?
                .parse::<f64>()
                .map_err(|_| Error("BLS value is not numeric".into()))?;
            let vintage = format!(
                "{} {}",
                point
                    .get("year")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown"),
                point
                    .get("periodName")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            );
            Ok((number, vintage, format!("{LAUS_URL}{id}")))
        })();
        values.insert(id.to_owned(), decoded);
    }
    Ok(values)
}

/// Decodes the selected county row from a QCEW CSV response.
///
/// # Errors
/// Returns an error when required columns or a numeric wage are missing.
pub fn decode_qcew(bytes: &[u8], county: &str, source_url: String) -> Result<Observation, Error> {
    let mut reader = csv::Reader::from_reader(bytes);
    let headers = reader
        .headers()
        .map_err(|error| Error(error.to_string()))?
        .clone();
    let area_index = headers
        .iter()
        .position(|header| header == "area_fips")
        .ok_or_else(|| Error("QCEW response missing area_fips".into()))?;
    let value_index = headers
        .iter()
        .position(|header| header == "avg_wkly_wage")
        .ok_or_else(|| Error("QCEW response missing avg_wkly_wage".into()))?;
    let rows = reader
        .records()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| Error(error.to_string()))?;
    let row = rows
        .iter()
        .find(|row| row.get(area_index) == Some(county))
        .or_else(|| rows.first())
        .ok_or_else(|| Error("QCEW area file has no rows".into()))?;
    let value = row
        .get(value_index)
        .ok_or_else(|| Error("QCEW wage value missing".into()))?
        .parse::<f64>()
        .map_err(|_| Error("QCEW wage is not numeric".into()))?;
    Ok((value, "2025 Q4 (preliminary/revised)".into(), source_url))
}

/// Decodes a CES monthly series and converts employment from thousands to persons.
///
/// # Errors
/// Returns an error when the response is invalid or has no monthly value.
pub fn decode_ces(bytes: &[u8], series_id: &str) -> Result<Observation, Error> {
    let document: Value =
        serde_json::from_slice(bytes).map_err(|error| Error(format!("BLS JSON: {error}")))?;
    if document.get("status").and_then(Value::as_str) != Some("REQUEST_SUCCEEDED") {
        let message = document
            .get("message")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(Value::as_str)
            .unwrap_or("no details");
        return Err(Error(format!("BLS API unavailable: {message}")));
    }
    let point = document
        .pointer("/Results/series/0/data")
        .and_then(Value::as_array)
        .and_then(|data| {
            data.iter().find(|point| {
                point
                    .get("period")
                    .and_then(Value::as_str)
                    .is_some_and(|period| period.starts_with('M') && period != "M13")
            })
        })
        .ok_or_else(|| Error("CES series has no monthly observation".into()))?;
    let value = point
        .get("value")
        .and_then(Value::as_str)
        .ok_or_else(|| Error("CES value missing".into()))?
        .parse::<f64>()
        .map_err(|_| Error("CES value is not numeric".into()))?
        * 1000.0;
    let vintage = format!(
        "{} {}",
        point
            .get("year")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        point
            .get("periodName")
            .and_then(Value::as_str)
            .unwrap_or("")
    );
    Ok((value, vintage, format!("{LAUS_URL}{series_id}")))
}

fn hex(bytes: &[u8]) -> String {
    use fmt::Write as _;
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            let _ = write!(output, "{byte:02x}");
            output
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_synthetic_laus_fixture_and_skips_thirteenth_month() {
        let decoded = decode_laus(include_bytes!("../tests/fixtures/laus.json"))
            .expect("fixture should decode");
        assert_eq!(
            decoded["TEST00000000000003"]
                .as_ref()
                .expect("series observation")
                .0,
            8.7
        );
    }

    #[test]
    fn decodes_synthetic_qcew_fixture() {
        let result = decode_qcew(
            include_bytes!("../tests/fixtures/qcew.csv"),
            "99999",
            "fixture".into(),
        )
        .expect("fixture should decode");
        assert_eq!(result.0, 1234.0);
    }

    #[test]
    fn decodes_synthetic_ces_fixture_and_converts_units() {
        let decoded = decode_ces(
            include_bytes!("../tests/fixtures/ces.json"),
            "SMU12000000000000001",
        )
        .expect("CES fixture should decode");
        assert_eq!(decoded.0, 123_000.0);
        assert_eq!(decoded.1, "2099 January");
    }

    #[test]
    fn builds_series_ids_for_supported_endpoints() {
        assert_eq!(
            laus_series_id("county", "99999", "03").as_deref(),
            Some("LAUCN999990000000003")
        );
        assert_eq!(
            ces_series_id("state", "12", "12").expect("state series"),
            "SMU12000000000000001"
        );
    }

    #[test]
    fn debug_output_redacts_api_key() {
        let client =
            Client::new(Some("synthetic-secret".into()), None).expect("client should build");
        let debug = format!("{client:?}");
        assert!(
            !debug.contains("synthetic-secret"),
            "debug output must redact credentials"
        );
        assert!(
            debug.contains("[redacted]"),
            "debug output should make redaction visible"
        );
    }

    #[test]
    #[ignore = "requires live BLS endpoints"]
    fn live_bls_keyed_endpoints() {
        let Some(key) = std::env::var("BLS_API_KEY")
            .ok()
            .filter(|key| !key.trim().is_empty())
        else {
            eprintln!("credential_missing");
            return;
        };
        let client = Client::new(Some(key), None).expect("client");
        let _ = client
            .laus_values(&["LNS14000000".into()])
            .expect("LAUS live request");
        let _ = client
            .ces_value("state", "12", "12")
            .expect("CES live request");
        let _ = client.qcew_value("99999").expect("QCEW live request");
    }
}
