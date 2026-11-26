//! Blocking HTTP and response decoding for caller-selected SDMX observations.

use ephemeral_cache::EphemeralCache;
pub use ephemeral_cache::Source;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

/// Cached responses older than this are never served (live responses: hours).
pub const CACHE_TTL: Duration = Duration::from_hours(6);
/// The cache directory is pruned oldest-first to at most this many bytes.
pub const CACHE_CAP_BYTES: u64 = 8 * 1024 * 1024;

/// Agencies with request conventions used by this client.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Agency {
    /// Organisation for Economic Co-operation and Development.
    Oecd,
    /// Eurostat.
    Eurostat,
    /// International Monetary Fund.
    Imf,
    /// International Labour Organization.
    Ilo,
    /// UNESCO Institute for Statistics.
    Uis,
    /// European Central Bank.
    Ecb,
}

impl Agency {
    fn default_base(self) -> &'static str {
        match self {
            Self::Oecd => "https://sdmx.oecd.org/public/rest",
            Self::Eurostat => "https://ec.europa.eu/eurostat/api/dissemination/sdmx/2.1",
            Self::Imf => "https://www.imf.org/external/datamapper/api/v1",
            Self::Ilo => "https://rplumber.ilo.org/data/indicator",
            Self::Uis => "https://api.uis.unesco.org/api/public",
            Self::Ecb => "https://data-api.ecb.europa.eu/service",
        }
    }

    fn default_format(self) -> &'static str {
        match self {
            Self::Eurostat => "JSON",
            Self::Ecb => "csvdata",
            _ => "jsondata",
        }
    }
}

/// Builds a data URL using the agency's default conventions and caller values.
///
/// The caller supplies a complete flow and dimension key. No geography or
/// indicator interpretation is applied. IMF systematic-download and
/// commercial-reuse restrictions remain the caller's responsibility.
#[must_use]
pub fn data_url(
    agency: Agency,
    base: Option<&str>,
    flow: &str,
    key: &str,
    format: Option<&str>,
) -> String {
    let base = base
        .unwrap_or_else(|| agency.default_base())
        .trim_end_matches('/');
    let format = format.unwrap_or_else(|| agency.default_format());
    match agency {
        Agency::Eurostat => format!("{base}/data/{flow}/{key}?format={format}&lang=en"),
        Agency::Ecb => format!("{base}/data/{flow}/{key}?format={format}&lastNObservations=1"),
        Agency::Imf | Agency::Ilo | Agency::Uis => {
            format!("{base}/{flow}/{key}?format={format}")
        }
        Agency::Oecd => format!(
            "{base}/data/{flow}/{key}?lastNObservations=1&dimensionAtObservation=AllDimensions&format={format}"
        ),
    }
}

/// Errors returned while requesting or decoding an SDMX response.
#[derive(Debug, Error)]
pub enum Error {
    /// The HTTP client could not be created.
    #[error("HTTP client setup failed: {0}")]
    Client(#[from] reqwest::Error),
    /// The server returned an unsuccessful status code.
    #[error("upstream returned HTTP {status} for {url}")]
    HttpStatus { status: u16, url: String },
    /// A cache file could not be read or written.
    #[error("cache I/O failed: {0}")]
    Cache(#[from] std::io::Error),
    /// Cache-only mode was requested but no cached body exists.
    #[error("offline cache miss for {0}")]
    CacheMiss(String),
    /// A response body did not contain a supported observation.
    #[error("response decode failed: {0}")]
    Decode(String),
}

/// Controls whether requests may contact the upstream.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CacheMode {
    /// Read a cache entry when present and otherwise request the upstream.
    #[default]
    ReadThrough,
    /// Read only existing cache entries and never make a network request.
    Only,
}

/// Blocking client options.
#[derive(Clone, Debug)]
pub struct ClientBuilder {
    cache_dir: Option<PathBuf>,
    retries: u32,
    timeout: Duration,
    user_agent: String,
    cache_mode: CacheMode,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            cache_dir: None,
            retries: 3,
            timeout: Duration::from_secs(45),
            user_agent: format!("statsapi-sdmx/{}", env!("CARGO_PKG_VERSION")),
            cache_mode: CacheMode::ReadThrough,
        }
    }
}

impl ClientBuilder {
    /// Creates options with no cache, three retries, and a 45-second timeout.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Keeps a last-resort ephemeral cache under this caller-owned directory.
    #[must_use]
    pub fn cache_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Sets the number of retries after the initial request.
    #[must_use]
    pub const fn retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    /// Sets the per-request timeout.
    #[must_use]
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the HTTP user-agent string.
    #[must_use]
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Sets whether cache misses may contact the upstream.
    #[must_use]
    pub const fn cache_mode(mut self, mode: CacheMode) -> Self {
        self.cache_mode = mode;
        self
    }

    /// Builds a blocking client.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Client`] if the HTTP transport cannot be initialized.
    pub fn build(self) -> Result<Client, Error> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(self.user_agent)
            .timeout(self.timeout)
            .build()?;
        Ok(Client {
            http,
            cache: self
                .cache_dir
                .map(|dir| EphemeralCache::new(dir, CACHE_TTL, CACHE_CAP_BYTES)),
            retries: self.retries,
            cache_mode: self.cache_mode,
        })
    }
}

/// Blocking SDMX HTTP client: live API first, with an optional ephemeral
/// response cache (see [`CACHE_TTL`], [`CACHE_CAP_BYTES`]) as the last resort.
#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
    cache: Option<EphemeralCache>,
    retries: u32,
    cache_mode: CacheMode,
}

impl Client {
    /// Creates a client with default retry and timeout settings.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Client`] if the HTTP transport cannot be initialized.
    pub fn new() -> Result<Self, Error> {
        ClientBuilder::new().build()
    }

    /// Creates a client builder.
    #[must_use]
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// Fetches a URL from the live API, falling back to an unexpired cache
    /// entry only when the request fails.
    ///
    /// Successful responses are atomically written to the optional cache
    /// directory. Transient transport errors, HTTP 429, and 5xx responses
    /// receive bounded full-jitter retries; other statuses are returned.
    ///
    /// # Errors
    ///
    /// Returns an HTTP, cache, or transport error when the request fails.
    pub fn get(&self, url: &str) -> Result<Vec<u8>, Error> {
        self.get_sourced(url).map(|(body, _)| body)
    }

    /// Like [`Self::get`], also reporting the access path used.
    ///
    /// # Errors
    ///
    /// Returns an HTTP, cache, or transport error when the request fails.
    pub fn get_sourced(&self, url: &str) -> Result<(Vec<u8>, Source), Error> {
        let cached = || self.cache.as_ref().and_then(|cache| cache.get(url));
        if self.cache_mode == CacheMode::Only {
            return cached()
                .map(|body| (body, Source::Cache))
                .ok_or_else(|| Error::CacheMiss(url.to_string()));
        }
        match self.request(url) {
            Ok(body) => {
                if let Some(cache) = &self.cache {
                    cache.put(url, &body)?;
                }
                Ok((body, Source::Api))
            }
            Err(error) => cached().map(|body| (body, Source::Cache)).ok_or(error),
        }
    }

    fn request(&self, url: &str) -> Result<Vec<u8>, Error> {
        let mut attempt = 0;
        loop {
            match self.http.get(url).send() {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        match response.bytes() {
                            Ok(body) => {
                                return Ok(body.to_vec());
                            }
                            Err(error)
                                if attempt < self.retries
                                    && (error.is_timeout() || error.is_connect()) =>
                            {
                                pause(attempt, None);
                                attempt += 1;
                                continue;
                            }
                            Err(error) => return Err(error.into()),
                        }
                    }
                    if attempt < self.retries
                        && (status.as_u16() == 429 || status.is_server_error())
                    {
                        let retry_after = response
                            .headers()
                            .get(reqwest::header::RETRY_AFTER)
                            .and_then(|value| value.to_str().ok())
                            .and_then(|value| value.parse::<u64>().ok());
                        pause(attempt, retry_after);
                    } else {
                        return Err(Error::HttpStatus {
                            status: status.as_u16(),
                            url: url.to_string(),
                        });
                    }
                }
                Err(error)
                    if attempt < self.retries && (error.is_timeout() || error.is_connect()) =>
                {
                    pause(attempt, None);
                }
                Err(error) => return Err(error.into()),
            }
            attempt += 1;
        }
    }
}

fn pause(attempt: u32, retry_after: Option<u64>) {
    let cap_ms = 30_000_u64.min(250_u64.saturating_mul(1_u64 << attempt.min(7)));
    let jitter_ms = rand::random::<u64>() % (cap_ms + 1);
    std::thread::sleep(
        Duration::from_secs(retry_after.unwrap_or(0)).max(Duration::from_millis(jitter_ms)),
    );
}

/// Decodes common SDMX JSON, Eurostat JSON-stat, and SDMX CSV observations.
///
/// # Errors
///
/// Returns [`Error::Decode`] when the response has no supported observation.
pub fn parse_observation(body: &[u8]) -> Result<(Value, String), Error> {
    if let Ok(document) = serde_json::from_slice::<Value>(body) {
        if let Some(values) = document.get("value").and_then(Value::as_object) {
            if let Some(time_index) = document
                .pointer("/dimension/time/category/index")
                .and_then(Value::as_object)
            {
                let by_position: Vec<(&str, usize)> = time_index
                    .iter()
                    .filter_map(|(period, index)| {
                        Some((period.as_str(), usize::try_from(index.as_u64()?).ok()?))
                    })
                    .collect();
                if let Some((period, value)) =
                    by_position.iter().rev().find_map(|(period, index)| {
                        values
                            .get(&index.to_string())
                            .filter(|value| !value.is_null())
                            .map(|value| (*period, value.clone()))
                    })
                {
                    return Ok((value, period.to_string()));
                }
            }
        }
        return last_observation(&document)
            .ok_or_else(|| Error::Decode("SDMX response has no numeric observation".into()));
    }

    let text = std::str::from_utf8(body).map_err(|error| Error::Decode(error.to_string()))?;
    let mut lines = text.lines();
    let header = lines
        .next()
        .ok_or_else(|| Error::Decode("empty SDMX response".into()))?;
    let value_column = header
        .split(',')
        .position(|name| name.trim_matches('"').eq_ignore_ascii_case("OBS_VALUE"))
        .ok_or_else(|| Error::Decode("CSV response has no OBS_VALUE column".into()))?;
    let time_column = header
        .split(',')
        .position(|name| name.trim_matches('"').eq_ignore_ascii_case("TIME_PERIOD"));
    lines
        .rev()
        .find_map(|line| {
            let columns: Vec<&str> = line.split(',').map(|part| part.trim_matches('"')).collect();
            let value = columns.get(value_column)?.parse::<f64>().ok()?;
            let period = time_column
                .and_then(|index| columns.get(index))
                .copied()
                .unwrap_or("unknown")
                .to_string();
            Some((json!(value), period))
        })
        .ok_or_else(|| Error::Decode("CSV response has no numeric observation".into()))
}

fn last_observation(value: &Value) -> Option<(Value, String)> {
    match value {
        Value::Object(object) => {
            if let Some(observations) = object.get("observations").and_then(Value::as_object) {
                let candidate = observations.iter().rev().find_map(|(period, observation)| {
                    observation
                        .as_array()
                        .and_then(|parts| parts.first())
                        .filter(|item| !item.is_null())
                        .map(|item| (item.clone(), period.clone()))
                });
                if candidate.is_some() {
                    return candidate;
                }
            }
            object.values().rev().find_map(last_observation)
        }
        Value::Array(array) => array.iter().rev().find_map(last_observation),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn builds_agency_specific_urls() {
        assert_eq!(
            data_url(Agency::Eurostat, None, "demo", "A.B", None),
            "https://ec.europa.eu/eurostat/api/dissemination/sdmx/2.1/data/demo/A.B?format=JSON&lang=en"
        );
        assert_eq!(
            data_url(
                Agency::Ecb,
                Some("https://example.test/service/"),
                "ICP",
                "M.FR",
                None
            ),
            "https://example.test/service/data/ICP/M.FR?format=csvdata&lastNObservations=1"
        );
    }

    #[test]
    fn decodes_synthesized_csv_fixture() {
        let (value, period) =
            parse_observation(include_bytes!("../tests/fixtures/observations.csv"))
                .expect("fixture decodes");
        assert_eq!(value, json!(12.34));
        assert_eq!(period, "2099-01");
    }

    #[test]
    fn decodes_synthesized_json_fixture() {
        let fixture = include_bytes!("../tests/fixtures/observations.json");
        assert_eq!(
            parse_observation(fixture).expect("JSON fixture decodes"),
            (json!(56.78), "2099-02".into())
        );
    }

    #[test]
    fn decodes_synthesized_eurostat_json_stat_fixture() {
        let fixture = include_bytes!("../tests/fixtures/eurostat.json");
        assert_eq!(
            parse_observation(fixture).expect("JSON-stat fixture decodes"),
            (json!(45.67), "2099-02".into())
        );
    }

    #[test]
    fn retries_rate_limited_requests() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test server");
        let address = listener.local_addr().expect("local server address");
        let server = thread::spawn(move || {
            for (status, body) in [("429 Too Many Requests", ""), ("200 OK", "csv")] {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut request = [0; 1024];
                let _ = stream.read(&mut request).expect("read request");
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .expect("write response");
            }
        });

        let client = Client::builder().retries(1).build().expect("client builds");
        let response = client
            .get(&format!("http://{address}/data"))
            .expect("second request succeeds");
        server.join().expect("test server exits");
        assert_eq!(response, b"csv");
    }

    #[test]
    fn live_answer_is_api_then_cache_only_when_the_api_fails() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test server");
        let address = listener.local_addr().expect("local server address");
        let server = thread::spawn(move || {
            for (status, body) in [("200 OK", "csv"), ("500 Internal Server Error", "")] {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut request = [0; 1024];
                let _ = stream.read(&mut request).expect("read request");
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .expect("write response");
            }
        });
        let dir = std::env::temp_dir().join(format!("sdmx-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let client = Client::builder()
            .retries(0)
            .cache_dir(&dir)
            .build()
            .expect("client builds");
        let url = format!("http://{address}/data");
        assert_eq!(
            client.get_sourced(&url).expect("live"),
            (b"csv".to_vec(), Source::Api)
        );
        assert_eq!(
            client.get_sourced(&url).expect("fallback"),
            (b"csv".to_vec(), Source::Cache)
        );
        server.join().expect("test server exits");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn offline_expired_entry_is_a_miss() {
        let dir = std::env::temp_dir().join(format!("sdmx-expired-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        EphemeralCache::new(&dir, CACHE_TTL, CACHE_CAP_BYTES)
            .put("u", b"x")
            .expect("seed");
        let client = Client::builder()
            .cache_dir(&dir)
            .cache_mode(CacheMode::Only)
            .build()
            .expect("client builds");
        assert_eq!(client.get_sourced("u").expect("fresh").1, Source::Cache);
        for entry in std::fs::read_dir(&dir).expect("dir").flatten() {
            let file = std::fs::File::options()
                .write(true)
                .open(entry.path())
                .expect("open");
            file.set_modified(std::time::SystemTime::now() - CACHE_TTL - Duration::from_secs(60))
                .expect("mtime");
        }
        assert!(matches!(client.get_sourced("u"), Err(Error::CacheMiss(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
