//! Blocking client for Meta Social Connectedness Index files published on HDX.
use ephemeral_cache::EphemeralCache;
pub use ephemeral_cache::Source as AccessPath;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

const COUNTY_URL: &str = "https://data.humdata.org/dataset/e9988552-74e4-4ff4-943f-c782ac8bca87/resource/97dc352f-c9c5-47d6-a6ef-88709e14006c/download/us_counties.csv";
const ZCTA_URL: &str = "https://drive.usercontent.google.com/download?id=13gbCdgHD-xfkogDoSzcAGKhNLHMPy8tR&export=download&confirm=t";
const COUNTRY_URL: &str = "https://data.humdata.org/dataset/e9988552-74e4-4ff4-943f-c782ac8bca87/resource/652cf9c9-541f-47de-8d53-ff818062bd0c/download/country.csv";
const CENTROIDS_URL: &str = "https://www2.census.gov/geo/docs/maps-data/data/gazetteer/2025_Gazetteer/2025_Gaz_counties_national.zip";

/// Source file family to read.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Source {
    /// County to county SCI CSV.
    County,
    /// Country to country SCI CSV.
    Country,
    /// US ZIP Code Tabulation Area SCI ZIP archive.
    Zcta,
}

impl Source {
    /// Returns the upstream file URL for this source family.
    #[must_use]
    pub fn url(self) -> &'static str {
        match self {
            Self::County => COUNTY_URL,
            Self::Country => COUNTRY_URL,
            Self::Zcta => ZCTA_URL,
        }
    }
}

/// One SCI destination and its published scaled score.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Row {
    /// Destination geography identifier.
    pub friend: String,
    /// Published scaled SCI score.
    pub score: f64,
}

/// Client failures from file access, decoding, or cache operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<String> for Error {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

fn error(message: impl fmt::Display) -> Error {
    Error(message.to_string())
}

/// Rejects a download that is not a ZIP, naming the Google Drive quota page
/// that replaces the archive when the shared file's daily quota is spent.
fn check_zip_magic(mut file: &std::fs::File) -> Result<(), Error> {
    let mut head = Vec::new();
    file.by_ref()
        .take(4096)
        .read_to_end(&mut head)
        .map_err(error)?;
    if head.starts_with(b"PK\x03\x04") {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&head);
    if text.contains("Quota exceeded") {
        return Err(error(
            "source_quota_exceeded: Google Drive quota exceeded for the SCI ZCTA archive; retry after the quota resets",
        ));
    }
    Err(error(format!(
        "source_error: the SCI ZCTA download is not a ZIP archive (starts with {:?})",
        text.chars().take(40).collect::<String>()
    )))
}

/// A reusable SCI file client with an optional caller-owned cache directory.
#[derive(Clone, Debug, Default)]
pub struct Client {
    cache: Option<EphemeralCache>,
    source: Cell<AccessPath>,
    offline: bool,
}

impl Client {
    /// Creates a client without persistent caching. Network reads are enabled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enables the last-resort ephemeral cache (7-day TTL, 256 MiB cap, oldest
    /// first eviction) for filtered extracts in a caller-supplied directory.
    /// Downloaded archives are never kept.
    #[must_use]
    pub fn with_cache_dir(mut self, directory: impl Into<PathBuf>) -> Self {
        self.cache = Some(EphemeralCache::bulk(directory));
        self
    }

    /// Restricts reads to cached extracts.
    #[must_use]
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    /// Returns the path that answered the last read: `stream` (download filtered
    /// on the fly, nothing persisted) or `cache`.
    #[must_use]
    pub fn last_source(&self) -> AccessPath {
        self.source.get()
    }

    /// Reads and filters one geography's rows from the selected upstream file.
    ///
    /// # Errors
    /// Returns an error for unavailable cache/network access or malformed source data.
    pub fn rows(&self, source: Source, origin: &str) -> Result<Vec<Row>, Error> {
        match source {
            Source::Zcta => self.zcta_rows(origin),
            Source::County => self.fetch_rows(COUNTY_URL, origin),
            Source::Country => self.fetch_rows(COUNTRY_URL, origin),
        }
    }

    /// Computes the rank of a country's self-link mean against all country self-links.
    ///
    /// # Errors
    /// Returns an error for unavailable files or invalid/missing country rows.
    pub fn country_rank(&self, country: &str) -> Result<usize, Error> {
        let origin = format!("country-rank-{country}");
        if let Some(rank) = self.cached::<usize>(COUNTRY_URL, &origin) {
            return Ok(rank);
        }
        let response = self.get(COUNTRY_URL, Duration::from_mins(15))?;
        let mut csv = csv::Reader::from_reader(response);
        let headers = csv.headers().map_err(error)?.clone();
        let origin_idx = column(&headers, "user_region", "SCI country file")?;
        let friend_idx = column(&headers, "friend_region", "SCI country file")?;
        let score_idx = column(&headers, "scaled_sci", "SCI country file")?;
        let mut aggregate = HashMap::<String, (f64, usize)>::new();
        for result in csv.records() {
            let row = result.map_err(error)?;
            if row.get(origin_idx) != row.get(friend_idx) {
                continue;
            }
            let key = row
                .get(origin_idx)
                .ok_or("SCI country origin missing")?
                .to_owned();
            let score = row
                .get(score_idx)
                .and_then(|v| v.parse::<f64>().ok())
                .ok_or("SCI country score invalid")?;
            let item = aggregate.entry(key).or_default();
            item.0 += score;
            item.1 += 1;
        }
        let target = aggregate
            .get(country)
            .map(|(sum, count)| sum / count_as_f64(*count))
            .ok_or("SCI country has no self-link row")?;
        let rank = aggregate
            .values()
            .filter(|(sum, count)| sum / count_as_f64(*count) > target)
            .count()
            + 1;
        self.source.set(AccessPath::Stream);
        self.store(COUNTRY_URL, &origin, &rank);
        Ok(rank)
    }

    /// Loads Census county centroids used with the US county SCI files.
    ///
    /// # Errors
    /// Returns an error for unavailable data or malformed Gazetteer files.
    pub fn county_centroids(&self) -> Result<HashMap<String, (f64, f64)>, Error> {
        if let Some(rows) = self.cached(CENTROIDS_URL, "all-county-centroids") {
            return Ok(rows);
        }
        let bytes = self
            .get(CENTROIDS_URL, Duration::from_mins(15))?
            .bytes()
            .map_err(error)?;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(error)?;
        let index = (0..zip.len())
            .find_map(|i| {
                let item = zip.by_index(i).ok()?;
                item.name().ok()?.ends_with(".txt").then_some(i)
            })
            .ok_or("Gazetteer ZIP has no text file")?;
        let mut content = String::new();
        zip.by_index(index)
            .map_err(error)?
            .read_to_string(&mut content)
            .map_err(error)?;
        let mut lines = content.lines();
        let headers = lines
            .next()
            .ok_or("empty Gazetteer")?
            .split('|')
            .collect::<Vec<_>>();
        let index = |name: &str| {
            headers
                .iter()
                .position(|h| *h == name)
                .ok_or_else(|| Error(format!("Gazetteer missing {name}")))
        };
        let (county_idx, lat_idx, lon_idx) =
            (index("GEOID")?, index("INTPTLAT")?, index("INTPTLONG")?);
        let mut centroids = HashMap::new();
        for line in lines {
            let fields = line.split('|').collect::<Vec<_>>();
            if let (Some(county), Some(lat), Some(lon)) = (
                fields.get(county_idx),
                fields.get(lat_idx).and_then(|v| v.parse().ok()),
                fields.get(lon_idx).and_then(|v| v.parse().ok()),
            ) {
                centroids.insert((*county).to_owned(), (lat, lon));
            }
        }
        self.store(CENTROIDS_URL, "all-county-centroids", &centroids);
        Ok(centroids)
    }

    fn fetch_rows(&self, url: &str, origin: &str) -> Result<Vec<Row>, Error> {
        if let Some(rows) = self.cached_rows(url, origin) {
            return Ok(rows);
        }
        if self.offline {
            return Err("cache_miss: offline mode".into());
        }
        let response = self.get(url, Duration::from_mins(15))?;
        let rows = filtered_rows(response, origin)?;
        self.source.set(AccessPath::Stream);
        self.store(url, origin, &rows);
        Ok(rows)
    }

    fn zcta_rows(&self, origin: &str) -> Result<Vec<Row>, Error> {
        if let Some(rows) = self.cached_rows(ZCTA_URL, origin) {
            return Ok(rows);
        }
        if self.offline {
            return Err("cache_miss: offline mode".into());
        }
        let mut response = self.get(ZCTA_URL, Duration::from_hours(1))?;
        // The archive is spooled to the temp dir only to seek its central
        // directory, and is removed as soon as the shard is filtered.
        let key = Sha256::digest(origin.as_bytes());
        let suffix = key.iter().take(8).fold(String::new(), |mut out, byte| {
            use fmt::Write as _;
            let _ = write!(out, "{byte:02x}");
            out
        });
        let archive_path = std::env::temp_dir().join(format!(
            "statsapi-meta-sci-zcta-{}-{suffix}.zip",
            std::process::id()
        ));
        if let Some(parent) = archive_path.parent() {
            std::fs::create_dir_all(parent).map_err(error)?;
        }
        let mut archive = std::fs::File::create(&archive_path).map_err(error)?;
        std::io::copy(&mut response, &mut archive).map_err(error)?;
        drop(archive);
        let result = (|| {
            let file = std::fs::File::open(&archive_path).map_err(error)?;
            check_zip_magic(&file)?;
            let mut zip = zip::ZipArchive::new(file).map_err(error)?;
            let shard = origin.chars().next().ok_or("empty ZCTA origin")?;
            let shard_name = format!("us_zcta_shard_{shard}.csv");
            let index = (0..zip.len())
                .find_map(|i| {
                    let entry = zip.by_index(i).ok()?;
                    (entry.name().ok()? == shard_name).then_some(i)
                })
                .ok_or("SCI ZIP is missing the requested ZCTA shard")?;
            let rows = filtered_rows(zip.by_index(index).map_err(error)?, origin);
            rows
        })();
        let _ = std::fs::remove_file(archive_path);
        let rows = result?;
        self.source.set(AccessPath::Stream);
        self.store(ZCTA_URL, origin, &rows);
        Ok(rows)
    }

    fn get(&self, url: &str, timeout: Duration) -> Result<reqwest::blocking::Response, Error> {
        if self.offline {
            return Err("cache_miss: offline mode".into());
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(error)?;
        let mut last_error = None;
        for attempt in 0..4_u64 {
            match client.get(url).send() {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response)
                    if response.status().as_u16() == 429 || response.status().is_server_error() =>
                {
                    last_error = Some(Error(format!("upstream returned {}", response.status())));
                }
                Ok(response) => {
                    return Err(Error(format!("upstream returned {}", response.status())))
                }
                Err(e) => last_error = Some(error(e)),
            }
            if attempt < 3 {
                let jitter = (u64::from(std::process::id()) + attempt * 37) % 101;
                std::thread::sleep(Duration::from_millis((100_u64 << attempt) + jitter));
            }
        }
        Err(last_error.unwrap_or_else(|| Error("upstream request failed".into())))
    }

    fn cache_key(url: &str, origin: &str) -> String {
        format!("{url}#{origin}")
    }

    fn cached<T: DeserializeOwned>(&self, url: &str, origin: &str) -> Option<T> {
        let bytes = self.cache.as_ref()?.get(&Self::cache_key(url, origin))?;
        let value = serde_json::from_slice(&bytes).ok()?;
        self.source.set(AccessPath::Cache);
        Some(value)
    }

    fn cached_rows(&self, url: &str, origin: &str) -> Option<Vec<Row>> {
        self.cached::<Vec<Row>>(url, origin)
            .filter(|rows| !rows.is_empty())
    }

    fn store<T: Serialize>(&self, url: &str, origin: &str, value: &T) {
        if let (Some(cache), Ok(bytes)) = (&self.cache, serde_json::to_vec(value)) {
            let _ = cache.put(&Self::cache_key(url, origin), &bytes);
        }
    }
}

fn count_as_f64(count: usize) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

fn column(headers: &csv::StringRecord, name: &str, source: &str) -> Result<usize, Error> {
    headers
        .iter()
        .position(|header| header == name)
        .ok_or_else(|| Error(format!("{source} missing {name}")))
}

fn filtered_rows<R: Read>(reader: R, origin: &str) -> Result<Vec<Row>, Error> {
    let mut csv = csv::Reader::from_reader(reader);
    let headers = csv.headers().map_err(error)?.clone();
    let origin_idx = column(&headers, "user_region", "SCI file")?;
    let friend_idx = column(&headers, "friend_region", "SCI file")?;
    let score_idx = column(&headers, "scaled_sci", "SCI file")?;
    let mut rows = Vec::new();
    for result in csv.records() {
        let row = result.map_err(error)?;
        if row.get(origin_idx) != Some(origin) {
            continue;
        }
        let Some(friend) = row.get(friend_idx) else {
            continue;
        };
        let score = row
            .get(score_idx)
            .and_then(|s| s.parse::<f64>().ok())
            .ok_or("SCI scaled_sci is not numeric")?;
        rows.push(Row {
            friend: friend.to_owned(),
            score,
        });
    }
    Ok(rows)
}

/// Parses synthesized or upstream-format SCI CSV data for an origin geography.
///
/// # Errors
/// Returns an error when required columns or numeric scores are invalid.
pub fn decode_rows<R: Read>(reader: R, origin: &str) -> Result<Vec<Row>, Error> {
    filtered_rows(reader, origin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn spooled(bytes: &[u8]) -> std::fs::File {
        let path = std::env::temp_dir().join(format!(
            "statsapi-meta-sci-test-{}-{}.bin",
            std::process::id(),
            bytes.len()
        ));
        let mut file = std::fs::File::create(&path).expect("create");
        file.write_all(bytes).expect("write");
        drop(file);
        let file = std::fs::File::open(&path).expect("open");
        std::fs::remove_file(&path).expect("unlink");
        file
    }

    #[test]
    fn drive_quota_page_is_a_typed_reason_not_an_invalid_zip() {
        let page = spooled(b"<!DOCTYPE html><title>Google Drive - Quota exceeded</title>");
        let reason = check_zip_magic(&page)
            .expect_err("html is not a zip")
            .to_string();
        assert!(reason.starts_with("source_quota_exceeded"), "{reason}");
        let other = spooled(b"<html>nope");
        assert!(check_zip_magic(&other)
            .expect_err("html")
            .to_string()
            .starts_with("source_error"));
        assert!(check_zip_magic(&spooled(b"PK\x03\x04rest")).is_ok());
    }

    #[test]
    fn decodes_synthetic_upstream_layout() {
        let rows = decode_rows(
            include_bytes!("../tests/fixtures/county.csv").as_slice(),
            "00001",
        )
        .expect("synthesized fixture parses");
        assert_eq!(
            rows,
            vec![
                Row {
                    friend: "00001".into(),
                    score: 13.0
                },
                Row {
                    friend: "00003".into(),
                    score: 8.0
                }
            ]
        );
    }

    #[test]
    fn decodes_synthetic_country_and_zcta_fixtures() {
        let country = decode_rows(
            include_bytes!("../tests/fixtures/country.csv").as_slice(),
            "AA",
        )
        .expect("synthesized country fixture parses");
        assert_eq!(country.len(), 2);
        assert_eq!(country[0].score, 11.0);

        let zcta = decode_rows(
            include_bytes!("../tests/fixtures/us_zcta_shard_9.csv").as_slice(),
            "90001",
        )
        .expect("synthesized ZCTA shard fixture parses");
        assert_eq!(zcta.len(), 2);
        assert_eq!(zcta[1].friend, "90002");
    }

    #[test]
    fn reports_missing_columns() {
        let error =
            decode_rows("from,to,value\na,b,1\n".as_bytes(), "a").expect_err("invalid header");
        assert!(error.to_string().contains("user_region"));
    }

    #[test]
    fn cache_is_optional_for_offline_mode_errors() {
        let error = Client::new()
            .offline(true)
            .rows(Source::County, "00001")
            .expect_err("an absent offline cache is reported");
        assert!(error.to_string().contains("cache_miss"));
    }
}
