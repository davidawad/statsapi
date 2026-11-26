//! Blocking client for `OpenFEMA` public data endpoints.

#[cfg(not(feature = "cache"))]
pub use access::{Source, Trail};
#[cfg(feature = "cache")]
use ephemeral_cache::EphemeralCache;
#[cfg(feature = "cache")]
pub use ephemeral_cache::{Source, Trail};
use reqwest::StatusCode;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const USER_AGENT: &str =
    "Mozilla/5.0 (compatible; statsapi-fema/0.1; +https://github.com/davidawad/statsapi)";

const MAX_RETRIES: u32 = 3;
#[cfg(feature = "cache")]
/// Live responses stay in the last-resort cache for one hour.
const LIVE_TTL: Duration = Duration::from_secs(3600);
#[cfg(feature = "cache")]
/// Byte cap for the last-resort cache, evicted oldest-first.
const CACHE_CAP_BYTES: u64 = 32 * 1024 * 1024;

#[cfg(not(feature = "cache"))]
mod access {
    use std::sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum Source {
        Api,
        Stream,
        Cache,
    }

    impl Source {
        fn code(self) -> u8 {
            match self {
                Self::Api => 1,
                Self::Stream => 2,
                Self::Cache => 3,
            }
        }

        fn from_code(code: u8) -> Option<Self> {
            match code {
                1 => Some(Self::Api),
                2 => Some(Self::Stream),
                3 => Some(Self::Cache),
                _ => None,
            }
        }
    }

    #[derive(Clone, Debug, Default)]
    pub struct Trail(Arc<AtomicU8>);

    impl Trail {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn note(&self, source: Source) {
            self.0.fetch_max(source.code(), Ordering::Relaxed);
        }

        pub fn take(&self) -> Option<Source> {
            Source::from_code(self.0.swap(0, Ordering::Relaxed))
        }
    }
}

/// Errors returned while fetching `FEMA` responses.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The HTTP request failed.
    Http(reqwest::Error),
    /// `FEMA` returned a non-success status.
    Status { url: String, status: u16 },
    /// The configured response cache failed.
    Cache(std::io::Error),
    /// Offline mode could not find the requested response.
    CacheMiss(String),
    /// The response was not valid JSON.
    Decode(serde_json::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(error) => write!(f, "FEMA request failed: {error}"),
            Self::Status { url, status: 403 } => write!(
                f,
                "source_blocked: FEMA edge returned HTTP 403 for {url} (denied this client address or agent)"
            ),
            Self::Status { url, status } => write!(f, "FEMA returned HTTP {status} for {url}"),
            Self::Cache(error) => write!(f, "FEMA cache failed: {error}"),
            Self::CacheMiss(url) => write!(f, "offline cache miss for {url}"),
            Self::Decode(error) => write!(f, "FEMA response JSON is invalid: {error}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Cache(error)
    }
}

/// Blocking `OpenFEMA` client: live first, with an optional last-resort
/// ephemeral cache (1 h TTL, 32 MiB cap, oldest-first eviction).
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    cache_dir: Option<PathBuf>,
    offline: bool,
    trail: Trail,
}
impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}
impl Client {
    /// Creates a client with online access and no persistent cache.
    #[must_use]
    pub fn new() -> Self {
        // The FEMA edge (Akamai) refuses requests that carry no User-Agent.
        let http = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_default();
        Self {
            http,
            cache_dir: None,
            offline: false,
            trail: Trail::new(),
        }
    }
    /// Reports access paths into a caller-owned trail instead of this client's own.
    #[must_use]
    pub fn with_trail(mut self, trail: Trail) -> Self {
        self.trail = trail;
        self
    }
    /// The access paths (api|cache) this client has answered with.
    #[must_use]
    pub fn trail(&self) -> Trail {
        self.trail.clone()
    }
    /// Uses a caller-selected directory for the last-resort ephemeral cache.
    #[must_use]
    pub fn with_cache_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(path.as_ref().to_path_buf());
        self
    }
    /// Requires cached responses and disables network access.
    #[must_use]
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }
    /// Fetches a JSON response from an `OpenFEMA` or `FEMA` `ArcGIS` URL.
    ///
    /// # Errors
    /// Returns an error for HTTP, decoding, cache, or offline cache-miss failures.
    pub fn get_json(&self, url: &str) -> Result<serde_json::Value, Error> {
        #[cfg(feature = "cache")]
        let (bytes, source) = match &self.cache_dir {
            Some(dir) => EphemeralCache::new(dir, LIVE_TTL, CACHE_CAP_BYTES).answer(
                url,
                self.offline,
                || self.fetch(url),
            )?,
            None if self.offline => return Err(Error::CacheMiss(url.to_owned())),
            None => (self.fetch(url)?, Source::Api),
        };
        #[cfg(not(feature = "cache"))]
        let (bytes, source) = if self.offline {
            return Err(Error::CacheMiss(url.to_owned()));
        } else {
            (self.fetch(url)?, Source::Api)
        };
        self.trail.note(source);
        serde_json::from_slice(&bytes).map_err(Error::Decode)
    }
    fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        for attempt in 0..=MAX_RETRIES {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    return response
                        .bytes()
                        .map(|bytes| bytes.to_vec())
                        .map_err(Error::Http)
                }
                Ok(response) => {
                    let status = response.status();
                    if !(status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                        || attempt == MAX_RETRIES
                    {
                        return Err(Error::Status {
                            url: url.to_owned(),
                            status: status.as_u16(),
                        });
                    }
                    let jitter = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_or(0, |now| u64::from(now.subsec_millis()) % 250);
                    std::thread::sleep(Duration::from_millis(
                        250_u64.saturating_mul(1 << attempt) + jitter,
                    ));
                }
                Err(error) if attempt == MAX_RETRIES => return Err(Error::Http(error)),
                Err(_) => {
                    let jitter = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_or(0, |now| u64::from(now.subsec_millis()) % 250);
                    std::thread::sleep(Duration::from_millis(
                        250_u64.saturating_mul(1 << attempt) + jitter,
                    ));
                }
            }
        }
        Err(Error::CacheMiss(url.to_owned()))
    }
}
