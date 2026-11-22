//! Blocking client for the Urban Institute Education Data Portal.

use reqwest::blocking::Client as HttpClient;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{hash::BuildHasher, path::PathBuf, thread, time::Duration};

/// Portal API root.
pub const API_ROOT: &str = "https://educationdata.urban.org/api/v1";
/// Build an absolute Portal API URL from a documented path and query pairs.
#[must_use]
pub fn endpoint(path: &str, query: &[(&str, &str)]) -> String {
    let path = path.trim_start_matches('/');
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.extend_pairs(query.iter().copied());
    let query = serializer.finish();
    if query.is_empty() {
        format!("{API_ROOT}/{path}")
    } else {
        format!("{API_ROOT}/{path}?{query}")
    }
}

/// Client configuration. Caching is disabled unless `cache_dir` is supplied.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Optional caller-owned response cache directory.
    pub cache_dir: Option<PathBuf>,
    /// Fail on a cache miss instead of making an HTTP request.
    pub offline: bool,
    /// Number of retries after the initial request.
    pub max_retries: u32,
}

/// Errors returned while requesting or decoding portal data.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Request transport failed.
    #[error("portal request failed: {0}")]
    Transport(#[from] reqwest::Error),
    /// Portal returned a non-success status after retries.
    #[error("portal returned HTTP {status} for {url}")]
    Status { url: String, status: u16 },
    /// The cached or returned body was not valid JSON.
    #[error("invalid portal JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// Offline mode had no cached response.
    #[error("offline cache miss for {0}")]
    CacheMiss(String),
    /// Portal response exceeded the safety limit.
    #[error("portal response exceeds 25,000 rows")]
    TooManyRows,
    /// Reading or writing the optional cache failed.
    #[error("portal cache I/O failed: {0}")]
    CacheIo(#[from] std::io::Error),
}

/// Reusable Education Data Portal API client.
#[derive(Debug, Clone)]
pub struct Client {
    http: HttpClient,
    options: Options,
}

impl Client {
    /// Create a keyless client with default options and no cache.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn new() -> Result<Self, Error> {
        Self::with_options(Options::default())
    }

    /// Create a client with explicit cache, offline, and retry settings.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn with_options(options: Options) -> Result<Self, Error> {
        let http = HttpClient::builder()
            .user_agent(concat!(
                "statsapi-urban-education/",
                env!("CARGO_PKG_VERSION")
            ))
            .timeout(Duration::from_secs(45))
            .build()?;
        Ok(Self { http, options })
    }

    /// Fetch all pages from an absolute Education Data Portal API URL.
    ///
    /// The JSON layout follows the portal's `{ "results": [...], "next": ... }`
    /// pagination envelope. Requests retry transport failures, HTTP 429 and 5xx
    /// with bounded equal jitter.
    ///
    /// # Errors
    /// Returns an error for transport failures, unsuccessful HTTP statuses,
    /// invalid JSON, cache I/O failures, or responses over the row limit.
    pub fn rows(&self, url: &str) -> Result<Vec<Value>, Error> {
        let mut next = Some(url.to_owned());
        let mut all = Vec::new();
        while let Some(page) = next.take() {
            let payload = self.fetch(&page)?;
            all.extend(
                payload
                    .get("results")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            );
            next = payload
                .get("next")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if all.len() > 25_000 {
                return Err(Error::TooManyRows);
            }
        }
        Ok(all)
    }

    fn fetch(&self, url: &str) -> Result<Value, Error> {
        let cache_path = self.options.cache_dir.as_ref().map(|root| {
            let digest = Sha256::digest(url.as_bytes());
            root.join(format!("{digest:x}"))
        });
        if let Some(path) = &cache_path {
            if let Ok(body) = std::fs::read(path) {
                return Ok(serde_json::from_slice(&body)?);
            }
        }
        if self.options.offline {
            return Err(Error::CacheMiss(url.to_owned()));
        }

        let mut attempt = 0;
        loop {
            let result = self.http.get(url).send().and_then(|response| {
                let status = response.status();
                if status.is_success() {
                    response.bytes().map(|bytes| bytes.to_vec())
                } else {
                    response.error_for_status().map(|_| Vec::new())
                }
            });
            match result {
                Ok(body) => {
                    if let Some(path) = &cache_path {
                        std::fs::create_dir_all(path.parent().unwrap_or(path.as_path()))?;
                        let temp = path.with_extension(format!("tmp.{}", std::process::id()));
                        std::fs::write(&temp, &body)?;
                        std::fs::rename(temp, path)?;
                    }
                    return Ok(serde_json::from_slice(&body)?);
                }
                Err(error) => {
                    let retryable = error.is_timeout()
                        || error.is_connect()
                        || error.is_body()
                        || error.status().is_some_and(|status| {
                            status.as_u16() == 429 || status.is_server_error()
                        });
                    if !retryable || attempt >= self.options.max_retries {
                        if let Some(status) = error.status() {
                            return Err(Error::Status {
                                url: url.to_owned(),
                                status: status.as_u16(),
                            });
                        }
                        return Err(Error::Transport(error));
                    }
                    let cap = 100_u64.saturating_mul(1_u64 << attempt.min(8)).min(5_000);
                    let jitter = (std::collections::hash_map::RandomState::new().hash_one(url)
                        % cap.max(1))
                    .max(cap / 2);
                    thread::sleep(Duration::from_millis(jitter));
                    attempt += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("fixture is available")
    }

    #[test]
    fn decodes_synthetic_upstream_page_fixture() {
        let path = format!(
            "{}/tests/fixtures/finance_page.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let payload: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("fixture"))
            .expect("portal page JSON");
        assert_eq!(payload["results"][0]["exp_total"], 123_456.0);
        assert_eq!(payload["next"], Value::Null);
    }

    #[test]
    fn builds_portal_endpoint_urls() {
        assert_eq!(
            endpoint(
                "school-districts/ccd/directory/2024/",
                &[("fips", "34"), ("page_size", "5")]
            ),
            format!("{API_ROOT}/school-districts/ccd/directory/2024/?fips=34&page_size=5")
        );
    }

    #[tokio::test]
    async fn follows_portal_pagination_layout() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/schools/ccd/finance/2024/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture("finance_page.json")))
            .mount(&server)
            .await;
        let url = format!("{}/api/v1/schools/ccd/finance/2024/", server.uri());
        let rows = tokio::task::spawn_blocking(move || {
            Client::with_options(Options {
                max_retries: 0,
                ..Options::default()
            })
            .expect("client builds")
            .rows(&url)
        })
        .await
        .expect("blocking request completes")
        .expect("page decodes");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["exp_total"], 123_456.0);
    }

    #[test]
    #[ignore = "live Urban Institute Education Data Portal proof"]
    fn live_portal_request() {
        let client = Client::new().expect("client builds");
        let rows = client.rows(&format!(
            "{API_ROOT}/school-districts/ccd/directory/2024/?fips=34&page_size=1"
        ));
        assert!(rows.is_ok(), "{rows:?}");
    }
}
