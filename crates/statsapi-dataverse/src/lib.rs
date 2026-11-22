//! Blocking access to Dataverse dataset metadata and files.

use reqwest::{blocking::Client as HttpClient, StatusCode, Url};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_RETRIES: u32 = 3;

/// A Dataverse guestbook response submitted when a dataset requires it.
#[derive(Clone, Debug, Serialize)]
pub struct GuestbookResponse {
    /// The downloader's accurate name.
    pub name: String,
    /// The downloader's accurate email address.
    pub email: String,
    /// The downloader's accurate institution or affiliation.
    pub institution: String,
    /// The downloader's accurate position.
    pub position: String,
    /// Answers to installation-specific guestbook questions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<GuestbookAnswer>,
}

/// One answer to an installation-specific Dataverse guestbook question.
#[derive(Clone, Debug, Serialize)]
pub struct GuestbookAnswer {
    /// The question identifier supplied by Dataverse.
    pub id: u64,
    /// The answer value expected by that question.
    pub value: serde_json::Value,
}

/// Errors returned by the client.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The base server URL was invalid.
    InvalidBaseUrl(String),
    /// A transport error occurred.
    Http(reqwest::Error),
    /// Dataverse returned an unsuccessful HTTP status.
    Status { url: String, status: u16 },
    /// A response could not be decoded as JSON.
    Decode(serde_json::Error),
    /// Optional cache I/O failed.
    Cache(std::io::Error),
    /// Offline mode has no cached response for the request.
    CacheMiss(String),
    /// Dataverse did not return a signed URL after guestbook submission.
    MissingSignedUrl,
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBaseUrl(url) => write!(formatter, "invalid Dataverse base URL: {url}"),
            Self::Http(error) => write!(formatter, "Dataverse HTTP request failed: {error}"),
            Self::Status { url, status } => {
                write!(formatter, "Dataverse returned HTTP {status} for {url}")
            }
            Self::Decode(error) => write!(formatter, "Dataverse response JSON is invalid: {error}"),
            Self::Cache(error) => write!(formatter, "Dataverse cache failed: {error}"),
            Self::CacheMiss(key) => write!(formatter, "offline cache miss for {key}"),
            Self::MissingSignedUrl => {
                formatter.write_str("Dataverse guestbook response contained no signed URL")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Decode(error) => Some(error),
            Self::Cache(error) => Some(error),
            Self::InvalidBaseUrl(_)
            | Self::Status { .. }
            | Self::CacheMiss(_)
            | Self::MissingSignedUrl => None,
        }
    }
}

/// A reusable client for one Dataverse installation.
#[derive(Clone, Debug)]
pub struct Client {
    server: Url,
    http: HttpClient,
    api_key: Option<String>,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl Client {
    /// Creates a client for `server`, such as `https://dataverse.harvard.edu`.
    ///
    /// The optional API key is supplied with [`Client::with_api_key`] or the
    /// `DATAVERSE_API_KEY` environment variable. No cache is used by default.
    ///
    /// # Errors
    ///
    /// Returns an error if `server` is not an HTTP or HTTPS URL.
    pub fn new(server: &str) -> Result<Self, Error> {
        let mut server =
            Url::parse(server).map_err(|_| Error::InvalidBaseUrl(server.to_owned()))?;
        if !matches!(server.scheme(), "http" | "https") {
            return Err(Error::InvalidBaseUrl(server.to_string()));
        }
        server.set_path(&format!("{}/api/", server.path().trim_end_matches('/')));
        let api_key = std::env::var("DATAVERSE_API_KEY").ok();
        let http = HttpClient::builder()
            .user_agent(concat!("statsapi-dataverse/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(90))
            .build()
            .map_err(Error::Http)?;
        Ok(Self {
            server,
            http,
            api_key,
            cache_dir: None,
            offline: false,
        })
    }

    /// Sets an API key used in the `X-Dataverse-key` request header.
    #[must_use]
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Uses `directory` for a content-addressed response cache.
    #[must_use]
    pub fn with_cache_dir(mut self, directory: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(directory.as_ref().to_path_buf());
        self
    }

    /// Requires cache hits and disables network access.
    #[must_use]
    pub fn offline(mut self) -> Self {
        self.offline = true;
        self
    }

    /// Fetches a JSON API route relative to `/api/`, including query parameters.
    ///
    /// # Errors
    ///
    /// Returns transport, status, cache, or JSON decoding errors.
    pub fn get_json(
        &self,
        route: &str,
        query: &[(&str, &str)],
    ) -> Result<serde_json::Value, Error> {
        let mut url = self.route_url(route)?;
        url.query_pairs_mut().extend_pairs(query.iter().copied());
        let key = format!("GET {url}");
        let bytes = self.request_bytes("GET", &url, None, &key)?;
        decode_json(&bytes)
    }

    /// Downloads a file by numeric file ID, optionally submitting its guestbook response.
    ///
    /// A guestbook response must identify the downloader accurately and answer the
    /// questions configured by that Dataverse installation.
    ///
    /// # Errors
    ///
    /// Returns transport, status, cache, JSON decoding, or missing signed URL errors.
    pub fn download_file(
        &self,
        file_id: u64,
        guestbook: Option<&GuestbookResponse>,
    ) -> Result<Vec<u8>, Error> {
        let route = format!("access/datafile/{file_id}");
        let url = self.route_url(&route)?;
        if let Some(guestbook) = guestbook {
            let guestbook_url = format!("{url}?gbrecs=true");
            let body = serde_json::to_vec(&serde_json::json!({"guestbookResponse": guestbook}))
                .map_err(Error::Decode)?;
            let key = format!("POST {guestbook_url} {}", String::from_utf8_lossy(&body));
            if let Some(bytes) = self.read_cache(&key)? {
                return Ok(bytes);
            }
            if self.offline {
                return Err(Error::CacheMiss(key));
            }
            let mut request = self
                .http
                .post(&guestbook_url)
                .header(reqwest::header::CONTENT_TYPE, "application/json");
            if let Some(key) = &self.api_key {
                request = request.header("X-Dataverse-key", key);
            }
            let response = self.send_with_retry(request.body(body))?;
            let signed = signed_url(&response)?;
            let bytes =
                self.request_bytes("GET", &signed, None, &format!("{key} signed-download"))?;
            self.write_cache(&key, &bytes)?;
            Ok(bytes)
        } else {
            let key = format!("GET {url}");
            self.request_bytes("GET", &url, None, &key)
        }
    }

    fn route_url(&self, route: &str) -> Result<Url, Error> {
        self.server
            .join(route.trim_start_matches('/'))
            .map_err(|_| Error::InvalidBaseUrl(route.to_owned()))
    }

    fn request_bytes(
        &self,
        method: &str,
        url: &Url,
        body: Option<Vec<u8>>,
        key: &str,
    ) -> Result<Vec<u8>, Error> {
        if let Some(bytes) = self.read_cache(key)? {
            return Ok(bytes);
        }
        if self.offline {
            return Err(Error::CacheMiss(key.to_owned()));
        }
        let mut request = match method {
            "POST" => self.http.post(url.clone()),
            _ => self.http.get(url.clone()),
        };
        if url.host_str() == self.server.host_str() {
            if let Some(key) = &self.api_key {
                request = request.header("X-Dataverse-key", key);
            }
        }
        if let Some(body) = body {
            request = request.body(body);
        }
        let bytes = self.send_with_retry(request)?;
        self.write_cache(key, &bytes)?;
        Ok(bytes)
    }

    fn send_with_retry(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> Result<Vec<u8>, Error> {
        let request = request.build().map_err(Error::Http)?;
        for attempt in 0..=MAX_RETRIES {
            match self.http.execute(request.try_clone().ok_or_else(|| {
                Error::InvalidBaseUrl("request body cannot be retried".to_owned())
            })?) {
                Ok(response) if response.status().is_success() => {
                    return response
                        .bytes()
                        .map(|bytes| bytes.to_vec())
                        .map_err(Error::Http)
                }
                Ok(response) => {
                    let status = response.status();
                    if !retryable(status) || attempt == MAX_RETRIES {
                        return Err(Error::Status {
                            url: request.url().to_string(),
                            status: status.as_u16(),
                        });
                    }
                }
                Err(error) if attempt == MAX_RETRIES => return Err(Error::Http(error)),
                Err(_) => {}
            }
            let jitter = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_millis()
                % 200;
            std::thread::sleep(Duration::from_millis(
                100 * (1 << attempt) + u64::from(jitter),
            ));
        }
        Err(Error::InvalidBaseUrl(
            "retry loop ended unexpectedly".to_owned(),
        ))
    }

    fn cache_path(&self, key: &str) -> Option<PathBuf> {
        self.cache_dir
            .as_ref()
            .map(|dir| dir.join(format!("{:x}", Sha256::digest(key.as_bytes()))))
    }

    fn read_cache(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        let Some(path) = self.cache_path(key) else {
            return Ok(None);
        };
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Error::Cache(error)),
        }
    }

    fn write_cache(&self, key: &str, bytes: &[u8]) -> Result<(), Error> {
        let Some(path) = self.cache_path(key) else {
            return Ok(());
        };
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        std::fs::create_dir_all(parent).map_err(Error::Cache)?;
        let temp = path.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&temp, bytes).map_err(Error::Cache)?;
        std::fs::rename(temp, path).map_err(Error::Cache)
    }
}

fn retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn decode_json(body: &[u8]) -> Result<serde_json::Value, Error> {
    serde_json::from_slice(body).map_err(Error::Decode)
}

fn signed_url(body: &[u8]) -> Result<Url, Error> {
    let response = decode_json(body)?;
    let url = response
        .pointer("/data/signedUrl")
        .and_then(serde_json::Value::as_str)
        .ok_or(Error::MissingSignedUrl)?;
    Url::parse(url).map_err(|_| Error::MissingSignedUrl)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_synthesized_dataset_metadata_layout() {
        let fixture = include_bytes!("../tests/fixtures/dataset.json");
        let response = decode_json(fixture).expect("fixture is JSON");
        assert_eq!(response["status"], "OK");
        assert_eq!(
            response["data"]["latestVersion"]["files"][0]["dataFile"]["filename"],
            "synthetic.csv"
        );
        assert_eq!(
            response["data"]["latestVersion"]["files"][0]["dataFile"]["id"],
            731
        );
    }

    #[test]
    fn decodes_synthesized_guestbook_signed_url_layout() {
        let fixture = include_bytes!("../tests/fixtures/guestbook-response.json");
        assert_eq!(
            signed_url(fixture).expect("signed URL is valid").as_str(),
            "https://files.example.test/download/synthetic"
        );
    }

    #[test]
    fn builds_api_routes_from_server_url() {
        let client = Client::new("https://example.test/root/").expect("valid URL");
        assert_eq!(
            client
                .route_url("datasets/:persistentId")
                .expect("route URL")
                .as_str(),
            "https://example.test/root/api/datasets/:persistentId"
        );
    }
}
