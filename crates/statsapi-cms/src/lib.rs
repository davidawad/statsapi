//! Blocking client for the CMS `data.cms.gov` data-api v1.
//!
//! The API is keyless. Each dataset is addressed by UUID and answers a JSON
//! array of flat string-valued rows; filters, sorting and column selection are
//! query parameters. Nothing is cached: every call is a live API request.

use reqwest::StatusCode;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Root of the data-api v1 dataset endpoints.
pub const API_BASE: &str = "https://data.cms.gov/data-api/v1/dataset";

/// Medicare Geographic Variation by National, State and County (multi-year).
pub const GEOGRAPHIC_VARIATION: &str = "6219697b-8f6c-4164-bed4-cd9317c58ebc";

/// Medicare Part D Opioid Prescribing Rates by Geography (multi-year).
pub const PART_D_OPIOID_GEOGRAPHY: &str = "94d00f36-73ce-4520-9b3f-83cd3cded25c";

const MAX_ATTEMPTS: usize = 4;

/// Errors returned while querying or decoding the data-api.
#[derive(Debug)]
pub enum Error {
    /// A network or HTTP failure that outlived the retries.
    Request(String),
    /// The body was not the expected JSON array of objects.
    Decode(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(message) => write!(formatter, "request failed: {message}"),
            Self::Decode(message) => write!(formatter, "decode failed: {message}"),
        }
    }
}

impl std::error::Error for Error {}

/// One flat data-api row (column name to string value).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row(BTreeMap<String, String>);

impl Row {
    /// Raw string value of a column, if present.
    #[must_use]
    pub fn text(&self, column: &str) -> Option<&str> {
        self.0.get(column).map(String::as_str)
    }

    /// Finite numeric value of a column; blank, suppressed (`*`) and
    /// non-numeric cells are `None`.
    #[must_use]
    pub fn number(&self, column: &str) -> Option<f64> {
        let trimmed = self.text(column)?.trim();
        trimmed
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
    }
}

/// Sort direction for a query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// Smallest first.
    Ascending,
    /// Largest first (newest year first).
    Descending,
}

/// A data-api query over one dataset.
#[derive(Clone, Debug)]
pub struct Query<'a> {
    /// Dataset UUID.
    pub dataset: &'a str,
    /// Exact-match `filter[COLUMN]=value` pairs.
    pub filters: Vec<(&'a str, &'a str)>,
    /// Optional `sort[COLUMN]` ordering.
    pub sort: Option<(&'a str, Order)>,
    /// Columns to return (all when empty).
    pub columns: Vec<&'a str>,
    /// Maximum rows.
    pub size: usize,
}

impl Query<'_> {
    /// The request URL query as `(name, value)` pairs.
    #[must_use]
    pub fn params(&self) -> Vec<(String, String)> {
        let mut params: Vec<(String, String)> = self
            .filters
            .iter()
            .map(|(column, value)| (format!("filter[{column}]"), (*value).to_owned()))
            .collect();
        if let Some((column, order)) = self.sort {
            let direction = match order {
                Order::Ascending => "ASC",
                Order::Descending => "DESC",
            };
            params.push((format!("sort[{column}]"), direction.to_owned()));
        }
        if !self.columns.is_empty() {
            params.push(("column".to_owned(), self.columns.join(",")));
        }
        params.push(("size".to_owned(), self.size.to_string()));
        params
    }
}

/// Geography level of a Medicare Geographic Variation or Part D row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// A county, keyed by 5-digit FIPS.
    County,
    /// A state, keyed by 2-digit FIPS.
    State,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Self::County => "County",
            Self::State => "State",
        }
    }
}

/// Reusable client.
#[derive(Clone, Debug)]
pub struct Client {
    base: String,
}

impl Default for Client {
    fn default() -> Self {
        Self {
            base: API_BASE.to_owned(),
        }
    }
}

impl Client {
    /// Client against a non-default endpoint root (tests, mirrors).
    #[must_use]
    pub fn with_base(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    /// URL a query resolves to, for provenance.
    #[must_use]
    pub fn dataset_url(&self, dataset: &str) -> String {
        format!("{}/{dataset}/data", self.base)
    }

    /// Run a query with jittered retry on 429, 5xx and transport errors.
    ///
    /// # Errors
    ///
    /// Returns an error when the request fails after retries or the body is
    /// not a JSON array of objects.
    pub fn rows(&self, query: &Query<'_>) -> Result<Vec<Row>, Error> {
        let http = reqwest::blocking::Client::builder()
            .user_agent("statsapi-cms/0.1")
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| Error::Request(error.to_string()))?;
        let url = self.dataset_url(query.dataset);
        let params = query.params();
        let mut last_error = String::new();
        for attempt in 0..MAX_ATTEMPTS {
            match http.get(&url).query(&params).send() {
                Ok(response) if response.status().is_success() => match response.text() {
                    Ok(body) => return decode_rows(&body),
                    Err(error) => last_error = error.to_string(),
                },
                Ok(response) => {
                    let status = response.status();
                    last_error = format!("HTTP {status}");
                    if !retryable_status(status) {
                        break;
                    }
                }
                Err(error) => last_error = error.to_string(),
            }
            if attempt + 1 < MAX_ATTEMPTS {
                retry_delay(attempt);
            }
        }
        Err(Error::Request(last_error))
    }

    /// Newest Geographic Variation row (all ages) for a county or state.
    ///
    /// # Errors
    ///
    /// Propagates [`Client::rows`] errors.
    pub fn geographic_variation(&self, level: Level, code: &str) -> Result<Option<Row>, Error> {
        let query = Query {
            dataset: GEOGRAPHIC_VARIATION,
            filters: vec![
                ("BENE_GEO_LVL", level.label()),
                ("BENE_GEO_CD", code),
                ("BENE_AGE_LVL", "All"),
            ],
            sort: Some(("YEAR", Order::Descending)),
            columns: Vec::new(),
            size: 1,
        };
        Ok(self.rows(&query)?.into_iter().next())
    }

    /// Newest overall (all prescribers) Part D opioid prescribing row for a
    /// county or state.
    ///
    /// # Errors
    ///
    /// Propagates [`Client::rows`] errors.
    pub fn opioid_prescribing(&self, level: Level, code: &str) -> Result<Option<Row>, Error> {
        let query = Query {
            dataset: PART_D_OPIOID_GEOGRAPHY,
            filters: vec![
                ("Prscrbr_Geo_Lvl", level.label()),
                ("Prscrbr_Geo_Cd", code),
                ("Breakout_Type", "Totals"),
            ],
            sort: Some(("Year", Order::Descending)),
            columns: Vec::new(),
            size: 1,
        };
        Ok(self.rows(&query)?.into_iter().next())
    }
}

/// Decode a data-api body (a JSON array of flat objects) into rows.
///
/// # Errors
///
/// Returns an error when the body is not a JSON array of objects.
pub fn decode_rows(body: &str) -> Result<Vec<Row>, Error> {
    let value: Value =
        serde_json::from_str(body).map_err(|error| Error::Decode(error.to_string()))?;
    let Value::Array(items) = value else {
        return Err(Error::Decode("expected a JSON array of rows".to_owned()));
    };
    items
        .into_iter()
        .map(|item| {
            let Value::Object(object) = item else {
                return Err(Error::Decode("row is not a JSON object".to_owned()));
            };
            Ok(Row(object
                .into_iter()
                .map(|(column, cell)| {
                    let text = match cell {
                        Value::String(text) => text,
                        Value::Null => String::new(),
                        other => other.to_string(),
                    };
                    (column, text)
                })
                .collect()))
        })
        .collect()
}

fn retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn retry_delay(attempt: usize) {
    let entropy = u64::from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos(),
    );
    let cap_ms = 250_u64.saturating_mul(1_u64 << attempt.min(6));
    thread::sleep(Duration::from_millis(entropy % (cap_ms + 1)));
}

#[cfg(test)]
mod tests {
    use super::*;

    const GV: &str = include_str!("../tests/fixtures/geographic_variation.json");

    #[test]
    fn decodes_flat_rows_and_numbers() {
        let rows = decode_rows(GV).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text("BENE_GEO_CD"), Some("34023"));
        assert_eq!(rows[0].number("ER_VISITS_PER_1000_BENES"), Some(512.25));
        assert_eq!(rows[0].number("MISSING"), None);
        assert_eq!(rows[0].number("BENE_GEO_DESC"), None);
    }

    #[test]
    fn rejects_non_array_bodies() {
        assert!(decode_rows("{\"errors\":[]}").is_err());
        assert!(decode_rows("[1]").is_err());
    }

    #[test]
    fn query_params_follow_data_api_syntax() {
        let query = Query {
            dataset: GEOGRAPHIC_VARIATION,
            filters: vec![("BENE_GEO_CD", "34")],
            sort: Some(("YEAR", Order::Descending)),
            columns: vec!["YEAR", "BENE_GEO_CD"],
            size: 1,
        };
        assert_eq!(
            query.params(),
            vec![
                ("filter[BENE_GEO_CD]".to_owned(), "34".to_owned()),
                ("sort[YEAR]".to_owned(), "DESC".to_owned()),
                ("column".to_owned(), "YEAR,BENE_GEO_CD".to_owned()),
                ("size".to_owned(), "1".to_owned()),
            ]
        );
    }
}
