//! Blocking client for `NOAA` `NCEI` `Storm Events` detail files.

#[cfg(not(feature = "cache"))]
pub use access::{Source, Trail};
#[cfg(feature = "cache")]
pub use ephemeral_cache::{Source, Trail};
use flate2::read::GzDecoder;
use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const INDEX: &str = "https://www.ncei.noaa.gov/pub/data/swdi/stormevents/csvfiles/";
const MAX_RETRIES: u32 = 3;

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
    }
}

/// Errors returned while locating or reading `NOAA` bulk files.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    Http(reqwest::Error),
    Status { url: String, status: u16 },
    Io(std::io::Error),
    Csv(csv::Error),
    MissingVintage(u16),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(e) => write!(f, "NOAA request failed: {e}"),
            Self::Status { url, status } => write!(f, "NOAA returned HTTP {status} for {url}"),
            Self::Io(e) => write!(f, "NOAA file read failed: {e}"),
            Self::Csv(e) => write!(f, "NOAA `CSV` is invalid: {e}"),
            Self::MissingVintage(y) => write!(f, "NOAA detail file is unavailable for {y}"),
        }
    }
}
impl std::error::Error for Error {}

/// A normalized county-level `NOAA` `Storm Events` detail record.
#[derive(Debug, Clone, PartialEq)]
pub struct StormEvent {
    pub year: u16,
    pub state_fips: String,
    pub county_fips: String,
    pub state: String,
    pub county: String,
    pub event_type: String,
    pub property_damage: f64,
    pub crop_damage: f64,
}

/// `NOAA` `NCEI` `Storm Events` client. The bulk files are streamed through
/// decompression and parsing; nothing is persisted.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    trail: Trail,
}
impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}
impl Client {
    /// Creates an online client.
    #[must_use]
    pub fn new() -> Self {
        Self {
            http: reqwest::blocking::Client::builder()
                .user_agent(
                    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/124.0 Safari/537.36",
                )
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new()),
            trail: Trail::new(),
        }
    }
    /// Reports access paths into a caller-owned trail instead of this client's own.
    #[must_use]
    pub fn with_trail(mut self, trail: Trail) -> Self {
        self.trail = trail;
        self
    }
    /// The access paths this client has answered with (always `stream`).
    #[must_use]
    pub fn trail(&self) -> Trail {
        self.trail.clone()
    }
    /// Downloads, decompresses and parses one annual detail file.
    ///
    /// # Errors
    /// Returns errors for network, missing-vintage, cache, decompression, or `CSV` failures.
    pub fn details(&self, year: u16) -> Result<Vec<StormEvent>, Error> {
        let listing = self.get(INDEX)?;
        let prefix = format!("StormEvents_details-ftp_v1.0_d{year}_c");
        let listing_text = String::from_utf8_lossy(&listing);
        let name = listing_text
            .lines()
            .filter_map(|line| {
                let start = line.find(&prefix)?;
                let tail = &line[start..];
                let end = tail.find(".csv.gz")? + 7;
                Some(&tail[..end])
            })
            .max()
            .ok_or(Error::MissingVintage(year))?
            .to_owned();
        let url = format!("{INDEX}{name}");
        let mut decompressor = GzDecoder::new(self.open(&url)?);
        let mut output = Vec::new();
        decompressor.read_to_end(&mut output).map_err(Error::Io)?;
        self.trail.note(Source::Stream);
        parse_details(&output)
    }
    fn get(&self, url: &str) -> Result<Vec<u8>, Error> {
        let mut bytes = Vec::new();
        self.open(url)?.read_to_end(&mut bytes).map_err(Error::Io)?;
        self.trail.note(Source::Stream);
        Ok(bytes)
    }
    fn open(&self, url: &str) -> Result<reqwest::blocking::Response, Error> {
        for attempt in 0..=MAX_RETRIES {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    return Ok(response);
                }
                Ok(response) => {
                    let status = response.status();
                    if !(status == reqwest::StatusCode::TOO_MANY_REQUESTS
                        || status.is_server_error())
                        || attempt == MAX_RETRIES
                    {
                        return Err(Error::Status {
                            url: url.to_owned(),
                            status: status.as_u16(),
                        });
                    }
                }
                Err(error) if attempt == MAX_RETRIES => return Err(Error::Http(error)),
                Err(_) => {}
            }
            let jitter = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |now| u64::from(now.subsec_millis()) % 250);
            std::thread::sleep(Duration::from_millis(
                250_u64.saturating_mul(1 << attempt) + jitter,
            ));
        }
        unreachable!("retry loop returns on its final attempt")
    }
}

/// Parses `NOAA`'s documented detail `CSV` layout and keeps the fields used by place summaries.
///
/// # Errors
/// Returns a `CSV` error when the required headers or rows are malformed.
pub fn parse_details(bytes: &[u8]) -> Result<Vec<StormEvent>, Error> {
    let mut reader = csv::Reader::from_reader(bytes);
    let headers = reader.headers().map_err(Error::Csv)?.clone();
    let index = |name: &str| {
        headers.iter().position(|h| h == name).ok_or_else(|| {
            Error::Csv(csv::Error::from(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("missing NOAA field {name}"),
            )))
        })
    };
    let (
        i_year,
        i_state_fips,
        i_county_fips,
        i_state,
        i_county,
        i_event,
        property_index,
        crop_index,
    ) = (
        index("YEAR")?,
        index("STATE_FIPS")?,
        index("CZ_FIPS")?,
        index("STATE")?,
        headers
            .iter()
            .position(|h| h == "CZ_NAME_STR" || h == "CZ_NAME")
            .ok_or_else(|| {
                Error::Csv(csv::Error::from(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "missing NOAA field CZ_NAME or CZ_NAME_STR",
                )))
            })?,
        index("EVENT_TYPE")?,
        index("DAMAGE_PROPERTY")?,
        index("DAMAGE_CROPS")?,
    );
    reader
        .records()
        .map(|row| {
            let row = row.map_err(Error::Csv)?;
            let number = |i: usize| row.get(i).map_or(0.0, parse_damage);
            let year = row
                .get(i_year)
                .and_then(|s| s.parse().ok())
                .unwrap_or_default();
            Ok(StormEvent {
                year,
                state_fips: row.get(i_state_fips).unwrap_or("").to_owned(),
                county_fips: row.get(i_county_fips).unwrap_or("").to_owned(),
                state: row.get(i_state).unwrap_or("").to_owned(),
                county: row.get(i_county).unwrap_or("").to_owned(),
                event_type: row.get(i_event).unwrap_or("").to_owned(),
                property_damage: damage(number(property_index)),
                crop_damage: damage(number(crop_index)),
            })
        })
        .collect()
}
fn damage(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}
fn parse_damage(value: &str) -> f64 {
    let value = value.trim();
    let (digits, multiplier) = match value.chars().last() {
        Some('K' | 'k') => (&value[..value.len() - 1], 1_000.0),
        Some('M' | 'm') => (&value[..value.len() - 1], 1_000_000.0),
        Some('B' | 'b') => (&value[..value.len() - 1], 1_000_000_000.0),
        _ => (value, 1.0),
    };
    digits
        .parse::<f64>()
        .map_or(0.0, |number| number * multiplier)
}
