//! Caller-configured clients for api.census.gov and Census Reporter ACS data.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use thiserror::Error;
use url::Url;

/// Census API root.
pub const API_BASE: &str = "https://api.census.gov/data";
/// Census Reporter data endpoint.
pub const REPORTER_BASE: &str = "https://api.censusreporter.org/1.0/data/show";

/// Errors returned by the Census clients and parsers.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// Network or HTTP failure.
    #[error("Census request failed: {0}")]
    Request(String),
    /// A response could not be decoded.
    #[error("Census response is invalid: {0}")]
    Decode(String),
    /// The caller supplied an invalid query.
    #[error("Census query is invalid: {0}")]
    Invalid(String),
    /// The API requires a key.
    #[error("Census API credential required")]
    Credential,
}

/// A redacted Census API key held by the caller.
#[derive(Clone)]
pub struct ApiKey(String);

impl ApiKey {
    /// Creates a key, rejecting blank values.
    pub fn new(key: impl Into<String>) -> Option<Self> {
        let key = key.into().trim().to_owned();
        (!key.is_empty()).then_some(Self(key))
    }
    /// Returns the key for request construction.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

/// The geographic clause used in a Census API query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Geography {
    /// The for clause.
    pub for_clause: String,
    /// The in clauses.
    pub in_clauses: Vec<String>,
}

/// One Census API query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    /// Dataset year.
    pub year: u16,
    /// Dataset path below data/{year}.
    pub dataset: String,
    /// Variables in API spelling, such as `B19001_001E`.
    pub variables: Vec<String>,
    /// Geography selection.
    pub geography: Geography,
}

impl Query {
    /// Builds a credential-bearing URL.
    ///
    /// # Panics
    ///
    /// Panics only if the fixed Census production endpoint is not a valid URL.
    #[expect(
        clippy::expect_used,
        reason = "The production endpoint is a compile-time constant maintained by this crate."
    )]
    #[must_use]
    pub fn url(&self, key: &ApiKey) -> String {
        let mut url = Url::parse(&format!("{}/{}/{}", API_BASE, self.year, self.dataset))
            .expect("Census production endpoint is a valid URL");
        let get = self.variables.join(",");
        {
            let mut pairs = url.query_pairs_mut();
            pairs
                .append_pair("get", &get)
                .append_pair("for", &self.geography.for_clause);
            for clause in &self.geography.in_clauses {
                pairs.append_pair("in", clause);
            }
            pairs.append_pair("key", key.as_str());
        }
        url.to_string()
    }
}

/// Blocking Census API client. It never reads a key from the environment.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    key: ApiKey,
}

impl Client {
    /// Creates a client with a caller-supplied key.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be initialized.
    pub fn new(key: ApiKey) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(40))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("statsapi-census/0.1 (+https://github.com/davidawad/statsapi)")
            .build()
            .map_err(|e| Error::Request(e.to_string()))?;
        Ok(Self { http, key })
    }
    /// Fetches one query and returns the raw body and credential-free URL.
    ///
    /// # Errors
    ///
    /// Returns an error when the request fails or the server rejects the query.
    pub fn fetch(&self, query: &Query) -> Result<(Vec<u8>, String), Error> {
        let url = query.url(&self.key);
        let response = self
            .http
            .get(&url)
            .send()
            .map_err(|e| Error::Request(e.to_string()))?;
        let status = response.status();
        let public = redact_key(&url);
        let bytes = response
            .bytes()
            .map_err(|e| Error::Request(e.to_string()))?
            .to_vec();
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Ok((bytes, public));
        }
        if !status.is_success() {
            return Err(Error::Request(format!("HTTP {status} for {public}")));
        }
        Ok((bytes, public))
    }
}

/// Parses the array-of-arrays response returned by api.census.gov.
///
/// # Errors
///
/// Returns an error when the response is not a Census API table or contains invalid JSON.
pub fn parse_api(body: &[u8]) -> Result<Vec<BTreeMap<String, Option<String>>>, Error> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(Vec::new());
    }
    let rows: Vec<Vec<Value>> = serde_json::from_slice(body).map_err(|_| {
        let head = String::from_utf8_lossy(body.get(..body.len().min(200)).unwrap_or_default())
            .to_lowercase();
        if head.contains("missing key")
            || head.contains("invalid key")
            || head.contains("key_signup")
        {
            Error::Credential
        } else {
            Error::Decode("not a Census API table".into())
        }
    })?;
    let Some(header) = rows.first() else {
        return Ok(Vec::new());
    };
    let names: Vec<String> = header
        .iter()
        .map(cell_text)
        .map(Option::unwrap_or_default)
        .collect();
    Ok(rows
        .into_iter()
        .skip(1)
        .map(|row| {
            names
                .iter()
                .enumerate()
                .map(|(i, name)| (name.clone(), row.get(i).and_then(cell_text)))
                .collect()
        })
        .collect())
}

/// Parses ACS estimate/margin pairs into a stable record map.
///
/// # Errors
///
/// Returns an error when the response is not a valid Census API table.
#[expect(
    clippy::type_complexity,
    reason = "The nested map mirrors ACS geography, variable, estimate, and margin dimensions."
)]
pub fn parse_acs(
    body: &[u8],
) -> Result<BTreeMap<String, BTreeMap<String, (Option<f64>, Option<f64>)>>, Error> {
    let rows = parse_api(body)?;
    let mut out = BTreeMap::new();
    for row in rows {
        let key = row
            .get("NAME")
            .and_then(Clone::clone)
            .unwrap_or_else(|| "unknown".into());
        let mut cells = BTreeMap::new();
        let mut stems = BTreeSet::new();
        for name in row
            .keys()
            .filter(|name| name.ends_with('E') || name.ends_with('M'))
        {
            stems.insert(name[..name.len() - 1].to_owned());
        }
        for stem in stems {
            let est = row
                .get(&format!("{stem}E"))
                .and_then(|v| v.as_deref())
                .and_then(clean);
            let moe = row
                .get(&format!("{stem}M"))
                .and_then(|v| v.as_deref())
                .and_then(clean);
            cells.insert(stem, (est, moe));
        }
        out.insert(key, cells);
    }
    Ok(out)
}

/// A keyless Census Reporter ACS client.
#[derive(Debug, Clone)]
pub struct Reporter {
    http: reqwest::blocking::Client,
    /// ACS release end year.
    pub year: u16,
}

impl Reporter {
    /// Creates a Reporter client for a pinned ACS release.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be initialized.
    pub fn new(year: u16) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(40))
            .user_agent("statsapi-census/0.1 (+https://github.com/davidawad/statsapi)")
            .build()
            .map_err(|e| Error::Request(e.to_string()))?;
        Ok(Self { http, year })
    }
    /// Fetches table IDs for Census Reporter geography IDs.
    ///
    /// # Errors
    ///
    /// Returns an error when the request fails or the server rejects the query.
    pub fn fetch(&self, tables: &[&str], geographies: &[&str]) -> Result<(Vec<u8>, String), Error> {
        let mut url = Url::parse(&format!("{REPORTER_BASE}/acs{}_5yr", self.year))
            .map_err(|error| Error::Request(error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("table_ids", &tables.join(","))
            .append_pair("geo_ids", &geographies.join(","));
        let url = url.to_string();
        let response = self
            .http
            .get(&url)
            .send()
            .map_err(|e| Error::Request(e.to_string()))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .map_err(|e| Error::Request(e.to_string()))?
            .to_vec();
        if !status.is_success() {
            return Err(Error::Request(format!("HTTP {status} for {url}")));
        }
        Ok((bytes, url))
    }
}

fn redact_key(url: &str) -> String {
    let Ok(mut parsed) = Url::parse(url) else {
        return url.to_owned();
    };
    parsed
        .query_pairs_mut()
        .clear()
        .append_pair("get", "redacted");
    parsed.to_string()
}

fn cell_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

fn clean(value: &str) -> Option<f64> {
    let number = value.trim().parse::<f64>().ok()?;
    (number > -100_000_000.0).then_some(number)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_synthetic_acs_rows() {
        let body = br#"[["NAME","B01003_001E","B01003_001M"],["Example","12","2"]]"#;
        assert_eq!(
            parse_acs(body).expect("fixture")["Example"]["B01003_001"],
            (Some(12.0), Some(2.0))
        );
    }
}
