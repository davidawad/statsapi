//! Blocking client for GLEIF LEI record counts.
//!
//! Every call goes to the live API and nothing is persisted: the API reports
//! the size of a filtered result set in `meta.pagination.total`, so a page of
//! one record answers a count.

use reqwest::StatusCode;
use serde_json::Value;
use std::time::Duration;

const BASE: &str = "https://api.gleif.org/api/v1/lei-records";
const MAX_ATTEMPTS: u32 = 4;

/// Errors returned while requesting or decoding GLEIF API responses.
#[derive(Debug)]
pub enum Error {
    /// The HTTP client could not be configured or a request failed.
    Http(reqwest::Error),
    /// The response body was not valid JSON.
    Json(serde_json::Error),
    /// The response lacked a required field.
    MissingField(&'static str),
    /// The server rejected the request with a non-retryable status.
    HttpStatus(StatusCode),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(error) => write!(formatter, "gleif request failed: {error}"),
            Self::Json(error) => write!(formatter, "invalid gleif JSON: {error}"),
            Self::MissingField(field) => write!(formatter, "gleif response missing {field}"),
            Self::HttpStatus(status) => write!(formatter, "gleif returned HTTP {status}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::MissingField(_) | Self::HttpStatus(_) => None,
        }
    }
}

/// A filtered LEI record count and the golden copy it was counted from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Count {
    /// Number of LEI records matching the filters.
    pub total: u64,
    /// Publish date of the GLEIF golden copy (`YYYY-MM-DD`), when reported.
    pub publish_date: Option<String>,
}

/// A blocking GLEIF API client.
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
            .user_agent(concat!("statsapi-gleif/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(Error::Http)?;
        Ok(Self { http })
    }

    /// Builds the count URL for `(filter name, value)` pairs, such as
    /// `("entity.legalAddress.country", "US")`. Values are URL-safe codes,
    /// statuses and `from..to` date ranges and are not escaped.
    #[must_use]
    pub fn count_url(filters: &[(&str, &str)]) -> String {
        let mut url = format!("{BASE}?page[size]=1&page[number]=1");
        for (name, value) in filters {
            url.push_str("&filter[");
            url.push_str(name);
            url.push_str("]=");
            url.push_str(value);
        }
        url
    }

    /// Counts LEI records matching all filters.
    ///
    /// # Errors
    /// Returns an error for transport failures, invalid JSON, or a response
    /// without a pagination total (GLEIF answers rejected filters that way).
    pub fn count(&self, filters: &[(&str, &str)]) -> Result<Count, Error> {
        let body = self.request_with_retry(&Self::count_url(filters))?;
        parse_count(&serde_json::from_slice(&body).map_err(Error::Json)?)
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
                        return Err(Error::HttpStatus(status));
                    }
                }
                Err(error) if attempt + 1 == MAX_ATTEMPTS => return Err(Error::Http(error)),
                Err(_) => {}
            }
            let jitter_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| u64::from(elapsed.subsec_nanos() % 97));
            std::thread::sleep(Duration::from_millis(200 * (1 << attempt) + jitter_ms));
        }
        unreachable!("retry loop returns on its final attempt")
    }
}

fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// Reads the pagination total and golden-copy date from a records response.
///
/// # Errors
/// Returns `MissingField` when the response has no pagination total.
pub fn parse_count(response: &Value) -> Result<Count, Error> {
    let total = response
        .pointer("/meta/pagination/total")
        .and_then(Value::as_u64)
        .ok_or(Error::MissingField("meta.pagination.total"))?;
    let publish_date = response
        .pointer("/meta/goldenCopy/publishDate")
        .and_then(Value::as_str)
        .map(|stamp| stamp.chars().take(10).collect());
    Ok(Count {
        total,
        publish_date,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_count_from_recorded_layout() {
        let payload: Value = serde_json::from_str(include_str!("../tests/fixtures/count.json"))
            .expect("fixture is valid JSON");
        assert_eq!(
            parse_count(&payload).expect("count parses"),
            Count {
                total: 366_012,
                publish_date: Some("2026-11-23".to_string())
            }
        );
    }

    #[test]
    fn rejected_filter_has_no_total() {
        let payload: Value = serde_json::from_str(include_str!("../tests/fixtures/error.json"))
            .expect("fixture is valid JSON");
        assert!(matches!(parse_count(&payload), Err(Error::MissingField(_))));
    }

    #[test]
    fn builds_filter_url() {
        assert_eq!(
            Client::count_url(&[("entity.legalAddress.country", "US")]),
            "https://api.gleif.org/api/v1/lei-records?page[size]=1&page[number]=1&filter[entity.legalAddress.country]=US"
        );
    }
}
