//! Blocking client for the ONS Customise-My-Data API (`api.beta.ons.gov.uk/v1`)
//! and the postcodes.io reverse-geocoder.
//!
//! Nothing is cached or written to disk: every call is a live request.

use reqwest::StatusCode;
use serde_json::Value;
use std::fmt::Write as _;
use std::time::Duration;

const ONS_BASE: &str = "https://api.beta.ons.gov.uk/v1";
const POSTCODES_BASE: &str = "https://api.postcodes.io";
const MAX_ATTEMPTS: u32 = 4;

/// Errors returned while requesting or decoding ONS and postcodes.io responses.
#[derive(Debug)]
pub enum Error {
    /// The HTTP client could not be configured or a request failed.
    Http(reqwest::Error),
    /// The response body was not valid JSON.
    Json(serde_json::Error),
    /// A successful JSON response lacked an expected field.
    MissingField(&'static str),
    /// The server answered with a non-retryable status; the body is the
    /// server's explanation when it sent one.
    HttpStatus(StatusCode, String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(error) => write!(formatter, "ons request failed: {error}"),
            Self::Json(error) => write!(formatter, "invalid ons JSON: {error}"),
            Self::MissingField(field) => write!(formatter, "ons response missing {field}"),
            Self::HttpStatus(status, body) if body.is_empty() => {
                write!(formatter, "ons returned HTTP {status}")
            }
            Self::HttpStatus(status, body) => {
                write!(formatter, "ons returned HTTP {status}: {body}")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::MissingField(_) | Self::HttpStatus(..) => None,
        }
    }
}

/// One observation: the time-dimension option id and the published number.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    /// Time option id, for example `2023` or `feb-apr-2021`.
    pub time: String,
    /// The observation value.
    pub value: f64,
    /// Coefficient of variation in percent, when the dataset publishes one.
    pub cv: Option<f64>,
}

/// A dataset version's observations endpoint, resolved from the dataset id.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Version {
    /// Absolute URL of the version, without a trailing slash.
    pub url: String,
}

/// A UK local-authority district found by reverse-geocoding a point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalAuthority {
    /// GSS code such as `E09000033`.
    pub code: String,
    /// Name such as `Westminster`.
    pub name: String,
}

/// A blocking client; it holds no state beyond the HTTP connection pool.
#[derive(Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
}

impl Client {
    /// Creates a client.
    ///
    /// # Errors
    /// Returns an error if the underlying HTTP client cannot be built.
    pub fn new() -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!("statsapi-ons/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(Error::Http)?;
        Ok(Self { http })
    }

    /// Resolves a dataset's latest version, or the latest version of one
    /// named edition.
    ///
    /// # Errors
    /// Returns an error for transport failures or a response without a
    /// `links.latest_version.href`.
    pub fn version(&self, dataset: &str, edition: Option<&str>) -> Result<Version, Error> {
        let url = match edition {
            Some(edition) => format!("{ONS_BASE}/datasets/{dataset}/editions/{edition}"),
            None => format!("{ONS_BASE}/datasets/{dataset}"),
        };
        parse_version(&self.get_json(&url)?)
    }

    /// Lists a dimension's option ids in the order ONS publishes them
    /// (newest first for time dimensions).
    ///
    /// # Errors
    /// Returns an error for transport or response failures.
    pub fn options(&self, version: &Version, dimension: &str) -> Result<Vec<String>, Error> {
        let url = format!("{}/dimensions/{dimension}/options?limit=1000", version.url);
        parse_options(&self.get_json(&url)?)
    }

    /// Builds the observations URL; `time=*` selects every period.
    #[must_use]
    pub fn observations_url(version: &Version, selection: &[(&str, &str)]) -> String {
        let mut url = format!("{}/observations", version.url);
        for (index, (dimension, option)) in selection.iter().enumerate() {
            url.push(if index == 0 { '?' } else { '&' });
            url.push_str(dimension);
            url.push('=');
            url.push_str(&encode(option));
        }
        url
    }

    /// Fetches observations for a selection that leaves at most one
    /// dimension as the `*` wildcard.
    ///
    /// # Errors
    /// Returns an error for transport or response failures; ONS answers an
    /// invalid selection with HTTP 400 and a one-line explanation.
    pub fn observations(
        &self,
        version: &Version,
        selection: &[(&str, &str)],
    ) -> Result<Vec<Observation>, Error> {
        parse_observations(&self.get_json(&Self::observations_url(version, selection))?)
    }

    /// Finds the local-authority district containing a point.
    ///
    /// Returns `Ok(None)` when no postcode lies within 2 km.
    ///
    /// # Errors
    /// Returns an error for transport or response failures.
    pub fn local_authority(&self, lat: f64, lon: f64) -> Result<Option<LocalAuthority>, Error> {
        let url = format!("{POSTCODES_BASE}/postcodes?lon={lon}&lat={lat}&limit=1&radius=2000");
        Ok(parse_local_authority(&self.get_json(&url)?))
    }

    fn get_json(&self, url: &str) -> Result<Value, Error> {
        let bytes = self.request_with_retry(url)?;
        serde_json::from_slice(&bytes).map_err(Error::Json)
    }

    fn request_with_retry(&self, url: &str) -> Result<Vec<u8>, Error> {
        for attempt in 0..MAX_ATTEMPTS {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    return response
                        .bytes()
                        .map(|bytes| bytes.to_vec())
                        .map_err(Error::Http);
                }
                Ok(response) => {
                    let status = response.status();
                    if !is_retryable(status) || attempt + 1 == MAX_ATTEMPTS {
                        let body = response.text().unwrap_or_default();
                        return Err(Error::HttpStatus(status, body.trim().to_string()));
                    }
                }
                Err(error) if attempt + 1 == MAX_ATTEMPTS => return Err(Error::Http(error)),
                Err(_) => {}
            }
            let jitter_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| u64::from(elapsed.subsec_nanos() % 97));
            std::thread::sleep(Duration::from_millis(100 * (1 << attempt) + jitter_ms));
        }
        unreachable!("retry loop returns on its final attempt")
    }
}

/// Picks the newest observation: the one whose time id comes first in
/// `time_order` (ONS lists time options newest first; observation order
/// itself is arbitrary).
#[must_use]
pub fn latest<'a>(
    observations: &'a [Observation],
    time_order: &[String],
) -> Option<&'a Observation> {
    observations
        .iter()
        .filter_map(|observation| {
            let rank = time_order
                .iter()
                .position(|time| *time == observation.time)?;
            Some((rank, observation))
        })
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, observation)| observation)
}

fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn encode(option: &str) -> String {
    let mut out = String::with_capacity(option.len());
    for byte in option.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'*') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

fn parse_version(response: &Value) -> Result<Version, Error> {
    let url = response
        .pointer("/links/latest_version/href")
        .and_then(Value::as_str)
        .ok_or(Error::MissingField("links.latest_version.href"))?;
    Ok(Version {
        url: url.trim_end_matches('/').to_string(),
    })
}

fn parse_options(response: &Value) -> Result<Vec<String>, Error> {
    let items = response
        .get("items")
        .and_then(Value::as_array)
        .ok_or(Error::MissingField("items"))?;
    Ok(items
        .iter()
        .filter_map(|item| item.get("option").and_then(Value::as_str))
        .map(str::to_string)
        .collect())
}

fn parse_observations(response: &Value) -> Result<Vec<Observation>, Error> {
    let rows = response
        .get("observations")
        .and_then(Value::as_array)
        .ok_or(Error::MissingField("observations"))?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let value = row.get("observation")?.as_str()?.trim().parse().ok()?;
            let time = row.pointer("/dimensions/Time/id")?.as_str()?.to_string();
            let cv = row
                .pointer("/metadata/CV")
                .and_then(Value::as_str)
                .and_then(|text| text.trim().parse().ok());
            Some(Observation { time, value, cv })
        })
        .collect())
}

fn parse_local_authority(response: &Value) -> Option<LocalAuthority> {
    let hit = response.get("result")?.as_array()?.first()?;
    let code = hit.pointer("/codes/admin_district")?.as_str()?;
    if code.is_empty() || code == "E99999999" {
        return None;
    }
    let name = hit
        .get("admin_district")
        .and_then(Value::as_str)
        .unwrap_or(code);
    Some(LocalAuthority {
        code: code.to_string(),
        name: name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(text: &str) -> Value {
        serde_json::from_str(text).expect("synthetic fixture is valid JSON")
    }

    #[test]
    fn decodes_observations_and_cv() {
        let rows = parse_observations(&fixture(include_str!(
            "../tests/fixtures/observations.json"
        )))
        .expect("observations parse");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].time, "2097");
        assert!((rows[1].value - 1234.5).abs() < f64::EPSILON);
        assert_eq!(rows[1].cv, Some(1.5));
        assert_eq!(rows[0].cv, None);
    }

    #[test]
    fn latest_follows_option_order_not_response_order() {
        let rows = parse_observations(&fixture(include_str!(
            "../tests/fixtures/observations.json"
        )))
        .expect("observations parse");
        let order: Vec<String> = ["2098", "2097", "2096"].map(str::to_string).into();
        assert_eq!(
            latest(&rows, &order).map(|row| row.time.as_str()),
            Some("2097")
        );
        assert!(latest(&rows, &[]).is_none());
    }

    #[test]
    fn decodes_version_and_options() {
        let version = parse_version(&fixture(include_str!("../tests/fixtures/dataset.json")))
            .expect("version parses");
        assert_eq!(
            version.url,
            "https://api.example.invalid/v1/datasets/syn/editions/e/versions/2"
        );
        let options = parse_options(&fixture(include_str!("../tests/fixtures/options.json")))
            .expect("options parse");
        assert_eq!(options, ["2098", "2097", "2096"]);
    }

    #[test]
    fn decodes_local_authority_and_rejects_placeholder_code() {
        let found =
            parse_local_authority(&fixture(include_str!("../tests/fixtures/postcodes.json")))
                .expect("local authority");
        assert_eq!(found.code, "E09000099");
        assert_eq!(found.name, "Synthetic Borough");
        assert!(parse_local_authority(&fixture(r#"{"result":null}"#)).is_none());
        assert!(parse_local_authority(&fixture(
            r#"{"result":[{"codes":{"admin_district":"E99999999"}}]}"#
        ))
        .is_none());
    }

    #[test]
    fn builds_encoded_observation_urls() {
        let version = Version {
            url: "https://x/v1/datasets/d/editions/e/versions/1".to_string(),
        };
        assert_eq!(
            Client::observations_url(&version, &[("time", "*"), ("age", "16+")]),
            "https://x/v1/datasets/d/editions/e/versions/1/observations?time=*&age=16%2B"
        );
    }
}
