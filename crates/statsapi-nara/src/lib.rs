//! Blocking access to the National Archives Catalog API v2.

use serde_json::Value;
use std::fmt;
use std::time::Duration;

/// NARA Catalog v2 search endpoint.
pub const SEARCH_URL: &str = "https://catalog.archives.gov/api/v2/records/search";

/// Errors returned by NARA Catalog requests.
#[derive(Debug)]
pub enum Error {
    /// The HTTP request failed.
    Http(reqwest::Error),
    /// The endpoint returned non-success status.
    Status(reqwest::StatusCode),
    /// The response was not valid JSON.
    Json(serde_json::Error),
    /// The search query cannot be encoded.
    Url(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(error) => write!(formatter, "NARA HTTP request failed: {error}"),
            Self::Status(status) => write!(formatter, "NARA HTTP status: {status}"),
            Self::Json(error) => write!(formatter, "NARA response is invalid JSON: {error}"),
            Self::Url(error) => write!(formatter, "NARA URL error: {error}"),
        }
    }
}

impl std::error::Error for Error {}

/// A caller-keyed, blocking NARA Catalog client.
#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
    api_key: String,
}

impl Client {
    /// Creates a client from the caller's NARA API key.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be initialized.
    pub fn new(api_key: impl Into<String>) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!("statsapi-nara/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(Error::Http)?;
        Ok(Self {
            http,
            api_key: api_key.into(),
        })
    }

    /// Builds the bounded v2 search URL.
    ///
    /// # Errors
    ///
    /// Returns an error when the fixed NARA endpoint cannot be parsed.
    pub fn search_url(query: &str, limit: usize) -> Result<String, Error> {
        let mut url =
            reqwest::Url::parse(SEARCH_URL).map_err(|error| Error::Url(error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("limit", &limit.to_string());
        Ok(url.into())
    }

    /// Searches the Catalog and returns its decoded JSON response.
    ///
    /// # Errors
    ///
    /// Returns an error when NARA rejects the request or returns invalid JSON.
    pub fn search(&self, query: &str, limit: usize) -> Result<Value, Error> {
        let url = Self::search_url(query, limit)?;
        self.http
            .get(url)
            .header("x-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .send()
            .map_err(Error::Http)?
            .error_for_status()
            .map_err(Error::Http)?
            .json()
            .map_err(Error::Http)
    }
}

/// Decodes a synthetic or cached Catalog response.
///
/// # Errors
///
/// Returns an error when the bytes are not valid JSON.
pub fn decode_response(bytes: &[u8]) -> Result<Value, Error> {
    serde_json::from_slice(bytes).map_err(Error::Json)
}
