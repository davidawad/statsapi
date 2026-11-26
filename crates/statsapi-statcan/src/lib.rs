//! Blocking client for the Statistics Canada Web Data Service (WDS).
//!
//! Every call is a live API request; nothing is written to disk. The WDS
//! sometimes answers with an HTML maintenance page and a 200 status, so a
//! body that is not JSON is retried like a transport error.

use reqwest::StatusCode;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

const BASE: &str = "https://www150.statcan.gc.ca/t1/wds/rest";
const MAX_ATTEMPTS: u32 = 5;
/// Number of dimension positions in a WDS coordinate.
const COORDINATE_LEN: usize = 10;

/// Errors returned while requesting or decoding WDS responses.
#[derive(Debug)]
pub enum Error {
    /// The HTTP client could not be configured or a request failed.
    Http(reqwest::Error),
    /// The server rejected a request with a non-retryable status.
    HttpStatus(StatusCode),
    /// The body stayed non-JSON (for example a maintenance page) after retries.
    NotJson,
    /// The WDS reported a failure for the request.
    Upstream(String),
    /// A successful response did not contain the expected field.
    MissingField(&'static str),
    /// The cube has no geography member with the requested code.
    UnknownGeography(String),
    /// The cube has no published value for the coordinate.
    NoValue,
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(error) => write!(formatter, "statcan request failed: {error}"),
            Self::HttpStatus(status) => write!(formatter, "statcan returned HTTP {status}"),
            Self::NotJson => write!(formatter, "statcan returned a non-JSON body"),
            Self::Upstream(message) => write!(formatter, "statcan reported failure: {message}"),
            Self::MissingField(field) => write!(formatter, "statcan response missing {field}"),
            Self::UnknownGeography(code) => {
                write!(formatter, "cube has no geography member {code}")
            }
            Self::NoValue => write!(formatter, "statcan has no published value"),
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

/// One member of a cube's Geography dimension.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeoMember {
    /// Position of the member in the dimension (used in coordinates).
    pub member_id: u64,
    /// English member name.
    pub name: String,
    /// Standard Geographical Classification code, when the member has one.
    pub code: Option<String>,
}

/// The newest published data point of one series.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    /// Series identifier (vector) the coordinate resolved to.
    pub vector_id: u64,
    /// Reference period, ISO date.
    pub ref_period: String,
    /// Published value.
    pub value: f64,
    /// Release timestamp reported by the WDS.
    pub release_time: Option<String>,
}

/// Blocking WDS client. Cube metadata is memoised in memory only.
#[derive(Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
    geographies: RefCell<HashMap<u64, Vec<GeoMember>>>,
}

impl Client {
    /// Creates a client.
    ///
    /// # Errors
    /// Returns an error if the underlying HTTP client cannot be built.
    pub fn new() -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!("statsapi-statcan/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(40))
            .build()
            .map_err(Error::Http)?;
        Ok(Self {
            http,
            geographies: RefCell::new(HashMap::new()),
        })
    }

    /// URL of the human table page for a product id, used as `source_url`.
    #[must_use]
    pub fn table_url(product_id: u64) -> String {
        format!("https://www150.statcan.gc.ca/t1/tbl1/en/tv.action?pid={product_id}01")
    }

    /// URL of the coordinate endpoint, for provenance.
    #[must_use]
    pub fn coordinate_url(product_id: u64, coordinate: &str) -> String {
        format!("{BASE}/getDataFromCubePidCoordAndLatestNPeriods (productId={product_id}, coordinate={coordinate})")
    }

    /// Geography members of a cube (memoised for the life of the client).
    ///
    /// # Errors
    /// Returns an error for transport or response failures.
    pub fn geographies(&self, product_id: u64) -> Result<Vec<GeoMember>, Error> {
        if let Some(members) = self.geographies.borrow().get(&product_id) {
            return Ok(members.clone());
        }
        let response = self.post("getCubeMetadata", &json!([{ "productId": product_id }]))?;
        let members = parse_geographies(&response)?;
        self.geographies
            .borrow_mut()
            .insert(product_id, members.clone());
        Ok(members)
    }

    /// Newest data point for a coordinate in a cube.
    ///
    /// # Errors
    /// Returns an error for transport, upstream, or empty-series failures.
    pub fn latest(&self, product_id: u64, coordinate: &str) -> Result<Observation, Error> {
        let body = json!([{
            "productId": product_id,
            "coordinate": pad_coordinate(coordinate),
            "latestN": 1,
        }]);
        let response = self.post("getDataFromCubePidCoordAndLatestNPeriods", &body)?;
        parse_latest(&response)
    }

    /// Newest data point for a geography, given as a coordinate template in
    /// which `{geo}` stands for the geography member id (for example
    /// `{geo}.7.1.1.1.1`). `code` is the SGC code (`35`) or `11124` for Canada.
    ///
    /// # Errors
    /// Returns an error when the cube has no such geography or no value.
    pub fn latest_for_geography(
        &self,
        product_id: u64,
        template: &str,
        code: &str,
    ) -> Result<(GeoMember, String, Observation), Error> {
        let member = self
            .geographies(product_id)?
            .into_iter()
            .find(|member| member.code.as_deref() == Some(code))
            .ok_or_else(|| Error::UnknownGeography(code.to_string()))?;
        let coordinate = pad_coordinate(&template.replace("{geo}", &member.member_id.to_string()));
        let observation = self.latest(product_id, &coordinate)?;
        Ok((member, coordinate, observation))
    }

    fn post(&self, method: &str, body: &Value) -> Result<Value, Error> {
        let url = format!("{BASE}/{method}");
        for attempt in 0..MAX_ATTEMPTS {
            let last = attempt + 1 == MAX_ATTEMPTS;
            match self.http.post(&url).json(body).send() {
                Ok(response) if response.status().is_success() => match response.json::<Value>() {
                    Ok(value) => return Ok(value),
                    Err(_) if last => return Err(Error::NotJson),
                    Err(_) => {}
                },
                Ok(response) => {
                    let status = response.status();
                    if !is_retryable(status) || last {
                        return Err(Error::HttpStatus(status));
                    }
                }
                Err(error) if last => return Err(Error::Http(error)),
                Err(_) => {}
            }
            let jitter_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| u64::from(elapsed.subsec_nanos() % 97));
            std::thread::sleep(Duration::from_millis(300 * (1 << attempt) + jitter_ms));
        }
        unreachable!("retry loop returns on its final attempt")
    }
}

fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// Pads a coordinate with trailing zero members to the WDS length of ten.
#[must_use]
pub fn pad_coordinate(coordinate: &str) -> String {
    let mut parts: Vec<&str> = coordinate.split('.').collect();
    while parts.len() < COORDINATE_LEN {
        parts.push("0");
    }
    parts.join(".")
}

/// Maps a Canadian province or territory abbreviation to its SGC code.
#[must_use]
pub fn province_sgc(abbreviation: &str) -> Option<&'static str> {
    Some(match abbreviation.to_ascii_uppercase().as_str() {
        "NL" => "10",
        "PE" => "11",
        "NS" => "12",
        "NB" => "13",
        "QC" => "24",
        "ON" => "35",
        "MB" => "46",
        "SK" => "47",
        "AB" => "48",
        "BC" => "59",
        "YT" => "60",
        "NT" => "61",
        "NU" => "62",
        _ => return None,
    })
}

/// SGC code the WDS uses for Canada as a whole.
pub const CANADA_CODE: &str = "11124";

fn success_object(response: &Value) -> Result<&Value, Error> {
    let first = response
        .get(0)
        .ok_or(Error::MissingField("response array"))?;
    if first.get("status").and_then(Value::as_str) != Some("SUCCESS") {
        let message = first
            .pointer("/object")
            .map_or_else(|| "FAILED".to_string(), ToString::to_string);
        return Err(Error::Upstream(message));
    }
    first.get("object").ok_or(Error::MissingField("object"))
}

fn parse_geographies(response: &Value) -> Result<Vec<GeoMember>, Error> {
    let object = success_object(response)?;
    let dimension = object
        .get("dimension")
        .and_then(Value::as_array)
        .and_then(|dimensions| {
            dimensions.iter().find(|dimension| {
                dimension.get("dimensionNameEn").and_then(Value::as_str) == Some("Geography")
            })
        })
        .ok_or(Error::MissingField("Geography dimension"))?;
    let members = dimension
        .get("member")
        .and_then(Value::as_array)
        .ok_or(Error::MissingField("Geography members"))?;
    Ok(members
        .iter()
        .filter_map(|member| {
            Some(GeoMember {
                member_id: member.get("memberId")?.as_u64()?,
                name: member.get("memberNameEn")?.as_str()?.to_string(),
                code: member
                    .get("classificationCode")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect())
}

fn parse_latest(response: &Value) -> Result<Observation, Error> {
    let object = success_object(response)?;
    let vector_id = object
        .get("vectorId")
        .and_then(Value::as_u64)
        .ok_or(Error::MissingField("vectorId"))?;
    let point = object
        .get("vectorDataPoint")
        .and_then(Value::as_array)
        .and_then(|points| {
            points
                .iter()
                .find(|point| point.get("value").is_some_and(Value::is_number))
        })
        .ok_or(Error::NoValue)?;
    Ok(Observation {
        vector_id,
        ref_period: point
            .get("refPer")
            .and_then(Value::as_str)
            .ok_or(Error::MissingField("refPer"))?
            .to_string(),
        value: point
            .get("value")
            .and_then(Value::as_f64)
            .ok_or(Error::NoValue)?,
        release_time: point
            .get("releaseTime")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(text: &str) -> Value {
        serde_json::from_str(text).expect("synthetic fixture is valid JSON")
    }

    #[test]
    fn decodes_geography_members_from_synthetic_metadata() {
        let members = parse_geographies(&fixture(include_str!("../tests/fixtures/metadata.json")))
            .expect("members parse");
        assert_eq!(members.len(), 3);
        assert_eq!(members[1].member_id, 4);
        assert_eq!(members[1].code.as_deref(), Some("35"));
        assert_eq!(members[2].code, None);
    }

    #[test]
    fn latest_skips_null_points() {
        let observation = parse_latest(&fixture(include_str!("../tests/fixtures/data.json")))
            .expect("observation parses");
        assert_eq!(observation.vector_id, 98765);
        assert_eq!(observation.ref_period, "2098-02-01");
        assert!((observation.value - 123.4).abs() < f64::EPSILON);
    }

    #[test]
    fn failed_status_is_an_upstream_error() {
        let failed = json!([{"status":"FAILED","object":{"responseStatusCode":2}}]);
        assert!(matches!(parse_latest(&failed), Err(Error::Upstream(_))));
    }

    #[test]
    fn coordinates_pad_to_ten_members() {
        assert_eq!(pad_coordinate("7.2"), "7.2.0.0.0.0.0.0.0.0");
        assert_eq!(pad_coordinate("1.7.1.1.1.1"), "1.7.1.1.1.1.0.0.0.0");
    }

    #[test]
    fn province_codes_resolve() {
        assert_eq!(province_sgc("on"), Some("35"));
        assert_eq!(province_sgc("ZZ"), None);
    }
}
