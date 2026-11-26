//! Blocking client and response parser for the EPA Air Quality System API.

use reqwest::Url;
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;

/// The EPA AQS data API root.
pub const API_BASE: &str = "https://aqs.epa.gov/data/api";

/// Errors returned by the AQS client or parser.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// The HTTP client could not be built or the request failed.
    #[error("EPA AQS request failed: {0}")]
    Request(String),
    /// AQS returned a failed header or malformed response.
    #[error("EPA AQS response is invalid: {0}")]
    Decode(String),
    /// The requested metric or geography is unsupported.
    #[error("EPA AQS query is invalid: {0}")]
    Invalid(String),
}

/// Caller-supplied EPA AQS credentials.
#[derive(Clone, Debug)]
pub struct Credentials {
    email: String,
    key: String,
}

impl Credentials {
    /// Creates credentials after rejecting empty values.
    ///
    /// # Errors
    ///
    /// Returns an error when either credential is blank.
    pub fn new(email: impl Into<String>, key: impl Into<String>) -> Result<Self, Error> {
        let email = email.into().trim().to_owned();
        let key = key.into().trim().to_owned();
        if email.is_empty() || key.is_empty() {
            return Err(Error::Invalid("email and key are required".into()));
        }
        Ok(Self { email, key })
    }
}

/// A request URL with its credential-free public form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    /// URL sent to AQS, including caller credentials.
    pub url: String,
    /// URL suitable for reporting to downstream users.
    pub public_url: String,
}

/// A blocking AQS client. It never reads environment variables or writes files.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    credentials: Credentials,
    base: String,
}

impl Client {
    /// Creates a client using the production AQS endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be initialized.
    pub fn new(credentials: Credentials) -> Result<Self, Error> {
        Self::with_base(API_BASE, credentials)
    }

    /// Creates a client against an alternate endpoint for tests.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be initialized.
    pub fn with_base(base: &str, credentials: Credentials) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(240))
            .user_agent("statsapi-epa-aqs/0.1 (+https://github.com/davidawad/statsapi)")
            .build()
            .map_err(|error| Error::Request(error.to_string()))?;
        Ok(Self {
            http,
            credentials,
            base: base.trim_end_matches('/').to_owned(),
        })
    }

    /// Builds a query for an AQS metric and resolved geography.
    ///
    /// # Errors
    ///
    /// Returns an error when the metric, geography, coordinates, or county identifier is invalid.
    pub fn query(
        &self,
        metric: &str,
        level: &str,
        geo_id: &str,
        latitude: Option<f64>,
        longitude: Option<f64>,
        year: i32,
    ) -> Result<Query, Error> {
        let (service, parameter) = match metric {
            "pm25_annual_mean" => ("annualData", "88101"),
            "ozone_4th_max_8hr" => ("annualData", "44201"),
            "days_over_aqi_100" | "nearest_pm25_daily" => ("dailyData", "88101"),
            _ => return Err(Error::Invalid(format!("unknown metric {metric}"))),
        };
        let target_year = if metric == "nearest_pm25_daily" {
            year - 1
        } else {
            year
        };
        let query_level = if metric == "nearest_pm25_daily" {
            "point"
        } else {
            level
        };
        let service = match (metric, level) {
            ("nearest_pm25_daily", _) | (_, "point") => format!("{service}/byBox"),
            (_, "county") => format!("{service}/byCounty"),
            (_, "cbsa") => format!("{service}/byCBSA"),
            _ => return Err(Error::Invalid(format!("unknown geography {level}"))),
        };
        let mut params = vec![
            ("param", parameter.to_owned()),
            ("bdate", format!("{target_year}0101")),
            ("edate", format!("{target_year}1231")),
        ];
        match query_level {
            "county" => {
                params.push((
                    "state",
                    geo_id
                        .get(..2)
                        .ok_or_else(|| Error::Invalid("county id is too short".into()))?
                        .to_owned(),
                ));
                params.push((
                    "county",
                    geo_id
                        .get(2..)
                        .ok_or_else(|| Error::Invalid("county id is too short".into()))?
                        .to_owned(),
                ));
            }
            "cbsa" => params.push(("cbsa", geo_id.to_owned())),
            "point" => {
                let (Some(lat), Some(lon)) = (latitude, longitude) else {
                    return Err(Error::Invalid("point queries require coordinates".into()));
                };
                params.extend([
                    ("minlat", format!("{}", lat - 0.25)),
                    ("maxlat", format!("{}", lat + 0.25)),
                    ("minlon", format!("{}", lon - 0.25)),
                    ("maxlon", format!("{}", lon + 0.25)),
                ]);
            }
            _ => return Err(Error::Invalid(format!("unknown geography {query_level}"))),
        }
        let make_url = |with_credentials: bool| -> Result<String, Error> {
            let mut url = Url::parse(&format!("{}/{service}", self.base))
                .map_err(|error| Error::Invalid(format!("invalid AQS base URL: {error}")))?;
            {
                let mut query = url.query_pairs_mut();
                if with_credentials {
                    query.append_pair("email", &self.credentials.email);
                    query.append_pair("key", &self.credentials.key);
                }
                for (name, value) in &params {
                    query.append_pair(name, value);
                }
            }
            Ok(url.to_string())
        };
        Ok(Query {
            url: make_url(true)?,
            public_url: make_url(false)?,
        })
    }

    /// Fetches and decodes one AQS JSON response.
    ///
    /// # Errors
    ///
    /// Returns an error when the request fails or the response is not valid JSON.
    pub fn fetch(&self, query: &Query) -> Result<Value, Error> {
        let response = self
            .http
            .get(&query.url)
            .send()
            .map_err(|e| Error::Request(e.to_string()))?;
        let response = response
            .error_for_status()
            .map_err(|e| Error::Request(e.to_string()))?;
        let bytes = response
            .bytes()
            .map_err(|e| Error::Request(e.to_string()))?;
        serde_json::from_slice(&bytes).map_err(|e| Error::Decode(e.to_string()))
    }
}

/// Returns the rows from a successful AQS document.
///
/// # Errors
///
/// Returns an error when the response header is unsuccessful or `Data` is not an array.
pub fn response_rows(value: &Value) -> Result<Vec<&Value>, Error> {
    if value.pointer("/Header/0/status").and_then(Value::as_str) != Some("Success") {
        return Err(Error::Decode(
            value
                .pointer("/Header/0/message")
                .and_then(Value::as_str)
                .unwrap_or("EPA AQS returned an error")
                .to_owned(),
        ));
    }
    match value.get("Data") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(rows)) => Ok(rows.iter().collect()),
        Some(_) => Err(Error::Decode("AQS response Data is not an array".into())),
    }
}

/// Reads a number encoded as either a JSON number or a string.
#[must_use]
pub fn numeric(row: &Value, field: &str) -> Option<f64> {
    row.get(field).and_then(|value| {
        value
            .as_str()
            .and_then(|s| s.parse().ok())
            .or_else(|| value.as_f64())
    })
}

/// Fetches a fully constructed AQS URL, preserving caller-supplied credentials.
///
/// # Errors
///
/// Returns an error when the request fails or the response is not valid JSON.
pub fn fetch_url(url: &str) -> Result<Value, Error> {
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(240))
        .user_agent("statsapi-epa-aqs/0.1 (+https://github.com/davidawad/statsapi)")
        .build()
        .map_err(|e| Error::Request(e.to_string()))?
        .get(url)
        .send()
        .map_err(|e| Error::Request(e.to_string()))?
        .error_for_status()
        .map_err(|e| Error::Request(e.to_string()))?;
    let bytes = response
        .bytes()
        .map_err(|e| Error::Request(e.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|e| Error::Decode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn query_redacts_credentials_and_parses_header_only_success() {
        let client = Client::new(Credentials::new("email", "secret").expect("fixture credentials"))
            .expect("client");
        let query = client
            .query("pm25_annual_mean", "county", "34023", None, None, 2025)
            .expect("query");
        assert!(!query.public_url.contains("secret"));
        assert_eq!(
            response_rows(&json!({"Header":[{"status":"Success"}]}))
                .expect("rows")
                .len(),
            0
        );
    }
}
