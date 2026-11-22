//! Blocking client for public U.S. Courts F-5A bankruptcy statistics.

use calamine::{open_workbook_auto_from_rs, Data, Reader};
use reqwest::blocking::Client as HttpClient;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use thiserror::Error;

const LISTING: &str = "https://www.uscourts.gov/data-table-topics/bankruptcy";

/// A county row as published in an F-5A workbook.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct County {
    /// Published county label.
    pub name: String,
    /// Total filings across chapters.
    pub all: Option<f64>,
    /// Chapter 7 filings.
    pub chapter7: Option<f64>,
    /// Chapter 13 filings.
    pub chapter13: Option<f64>,
    /// Business filings across chapters.
    pub business: Option<f64>,
    /// Nonbusiness filings across chapters.
    pub nonbusiness: Option<f64>,
}

/// Parsed F-5A reporting period, source URL, and county rows keyed by county FIPS.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Table {
    /// Reporting period in `YYYY-MM-DD` form.
    pub date: String,
    /// URL from which the workbook was retrieved.
    pub url: String,
    /// Rows keyed by five-digit county FIPS code.
    pub counties: BTreeMap<String, County>,
}

#[derive(Clone, Debug)]
struct TableLink {
    date: String,
    detail_url: String,
}

/// Errors returned while requesting or decoding F-5A data.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// An HTTP request failed after retries.
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    /// The server returned a non-success status.
    #[error("upstream returned HTTP {0}")]
    HttpStatus(reqwest::StatusCode),
    /// The response body was not valid UTF-8.
    #[error("upstream page was not UTF-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    /// Local cache I/O failed.
    #[error("cache I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// The requested vintage could not be found or parsed.
    #[error("invalid F-5A data: {0}")]
    Data(String),
    /// XLSX decoding failed.
    #[error("workbook decoding failed: {0}")]
    Workbook(String),
}

/// A blocking U.S. Courts bankruptcy statistics client.
///
/// The optional cache directory is owned by the caller. Requests use bounded
/// exponential backoff with jitter for transport failures, HTTP 429, and 5xx.
#[derive(Debug)]
pub struct Client {
    http: HttpClient,
    cache_dir: Option<PathBuf>,
    cache_only: bool,
}

impl Client {
    /// Create a client with no local cache.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be configured.
    pub fn new() -> Result<Self, Error> {
        Self::with_cache_dir(None::<PathBuf>)
    }

    /// Create a client with an optional caller-owned cache directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be configured.
    pub fn with_cache_dir(path: Option<impl AsRef<Path>>) -> Result<Self, Error> {
        Self::build(path.map(|p| p.as_ref().to_path_buf()), false)
    }

    /// Create a client that reads only from the caller-supplied cache directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be configured.
    pub fn offline_cache(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::build(Some(path.as_ref().to_path_buf()), true)
    }

    fn build(cache_dir: Option<PathBuf>, cache_only: bool) -> Result<Self, Error> {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(60))
            .user_agent(concat!(
                "statsapi-uscourts-bankruptcy/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()?;
        Ok(Self {
            http,
            cache_dir,
            cache_only,
        })
    }

    /// Download and parse the latest published F-5A table.
    ///
    /// # Errors
    ///
    /// Returns an error if a request, cache operation, or workbook decode fails.
    pub fn latest(&self) -> Result<Table, Error> {
        self.fetch(None)
    }

    /// Download and parse the F-5A reporting period with this date.
    ///
    /// # Errors
    ///
    /// Returns an error if a request, cache operation, or workbook decode fails.
    pub fn for_date(&self, date: &str) -> Result<Table, Error> {
        self.fetch(Some(date))
    }

    fn fetch(&self, date: Option<&str>) -> Result<Table, Error> {
        let listing = String::from_utf8(self.get(LISTING)?)?;
        let links = find_tables(&listing);
        let selected = match date {
            Some(date) => links
                .into_iter()
                .find(|link| link.date == date)
                .ok_or_else(|| Error::Data(format!("no F-5A vintage {date} is listed")))?,
            None => links
                .into_iter()
                .next()
                .ok_or_else(|| Error::Data("no F-5A table was listed".into()))?,
        };
        let page = String::from_utf8(self.get(&selected.detail_url)?)?;
        let url = xlsx_url(&page)
            .ok_or_else(|| Error::Data(format!("no XLSX link at {}", selected.detail_url)))?;
        parse_xlsx(&self.get(&url)?, &url, &selected.date)
    }

    fn get(&self, url: &str) -> Result<Vec<u8>, Error> {
        if let Some(root) = &self.cache_dir {
            let path = root.join(format!("{:x}", Sha256::digest(url.as_bytes())));
            if let Ok(bytes) = std::fs::read(&path) {
                return Ok(bytes);
            }
            if self.cache_only {
                return Err(Error::Data("offline cache miss".into()));
            }
            let bytes = self.get_with_retry(url)?;
            std::fs::create_dir_all(root)?;
            let temp = root.join(format!(
                ".{}.{}.tmp",
                path.file_name().unwrap_or_default().to_string_lossy(),
                std::process::id()
            ));
            std::fs::write(&temp, &bytes)?;
            std::fs::rename(temp, path)?;
            Ok(bytes)
        } else if self.cache_only {
            Err(Error::Data(
                "offline mode requires a cache directory".into(),
            ))
        } else {
            self.get_with_retry(url)
        }
    }

    fn get_with_retry(&self, url: &str) -> Result<Vec<u8>, Error> {
        for attempt in 0..4 {
            let response = self.http.get(url).send();
            match response {
                Ok(response) if response.status().is_success() => {
                    return Ok(response.bytes()?.to_vec());
                }
                Ok(response)
                    if !(response.status().as_u16() == 429
                        || response.status().is_server_error()) =>
                {
                    return Err(Error::HttpStatus(response.status()));
                }
                Ok(_) | Err(_) if attempt < 3 => {
                    let base_ms = 250_u64.saturating_mul(1_u64 << attempt);
                    thread::sleep(Duration::from_millis(
                        base_ms + u64::from(fastrand::u8(0..=250)),
                    ));
                }
                Ok(response) => return Err(Error::HttpStatus(response.status())),
                Err(error) => return Err(Error::Request(error)),
            }
        }
        Err(Error::Data("request retry limit reached".into()))
    }
}

fn find_tables(html: &str) -> Vec<TableLink> {
    let mut links = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("/data-news/data-tables/") {
        rest = &rest[start..];
        let Some(end) = rest.find('"') else { break };
        let path = &rest[..end];
        rest = &rest[end..];
        if !path.ends_with("/bankruptcy-filings/f-5a") {
            continue;
        }
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() < 8 {
            continue;
        }
        links.push(TableLink {
            date: format!("{}-{}-{}", parts[3], parts[4], parts[5]),
            detail_url: format!("https://www.uscourts.gov{path}"),
        });
    }
    links.sort_by(|a, b| b.date.cmp(&a.date));
    links.dedup_by(|a, b| a.detail_url == b.detail_url);
    links
}

fn xlsx_url(html: &str) -> Option<String> {
    let mut rest = html;
    while let Some(start) = rest.find("href=\"") {
        rest = &rest[start + 6..];
        let end = rest.find('"')?;
        let path = &rest[..end];
        rest = &rest[end + 1..];
        if path.to_ascii_lowercase().ends_with(".xlsx") {
            return Some(if path.starts_with("http") {
                path.into()
            } else {
                format!("https://www.uscourts.gov{path}")
            });
        }
    }
    None
}

fn text(cell: Option<&Data>) -> String {
    match cell {
        Some(Data::String(value)) => value.trim().to_string(),
        Some(Data::Float(value)) => format!("{value}"),
        Some(Data::Int(value)) => value.to_string(),
        Some(Data::Bool(value)) => value.to_string(),
        _ => String::new(),
    }
}

fn number(cell: Option<&Data>) -> Option<f64> {
    let value = text(cell).replace(',', "");
    if value.is_empty() || value == "-" {
        None
    } else {
        value.parse().ok()
    }
}

/// Decode an F-5A XLSX byte slice using the upstream workbook layout.
///
/// # Errors
///
/// Returns an error if the XLSX cannot be decoded or contains no county rows.
pub fn parse_xlsx(bytes: &[u8], url: &str, date: &str) -> Result<Table, Error> {
    let mut book = open_workbook_auto_from_rs(Cursor::new(bytes.to_vec()))
        .map_err(|error| Error::Workbook(error.to_string()))?;
    let range = book
        .worksheet_range_at(0)
        .ok_or_else(|| Error::Data("workbook has no first sheet".into()))?
        .map_err(|error| Error::Workbook(error.to_string()))?;
    let mut counties = BTreeMap::new();
    for row in range.rows().skip(7) {
        let code_raw = text(row.get(1));
        let code = code_raw.trim_end_matches('*').trim();
        if code.len() != 5
            || !code.bytes().all(|byte| byte.is_ascii_digit())
            || code_raw.ends_with('*')
        {
            continue;
        }
        counties.insert(
            code.to_string(),
            County {
                name: text(row.first()),
                all: number(row.get(2)),
                chapter7: number(row.get(3)),
                chapter13: number(row.get(5)),
                business: number(row.get(7)),
                nonbusiness: number(row.get(12)),
            },
        );
    }
    if counties.is_empty() {
        return Err(Error::Data("workbook contains no county rows".into()));
    }
    Ok(Table {
        date: date.to_string(),
        url: url.to_string(),
        counties,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synthetic_layout_fixture() {
        let data = include_bytes!("../tests/fixtures/f5a-synthetic.xlsx");
        let table = parse_xlsx(data, "fixture", "2030-03-31").expect("fixture parses");
        let county = table
            .counties
            .get("01001")
            .expect("synthetic county is present");
        assert_eq!(county.all, Some(123.0), "synthetic total is decoded");
        assert_eq!(
            county.chapter7,
            Some(80.0),
            "synthetic Chapter 7 total is decoded"
        );
    }

    #[test]
    fn finds_vintages_and_download_link() {
        let listing =
            r#"<a href="/data-news/data-tables/2030/03/31/bankruptcy-filings/f-5a">F-5A</a>"#;
        let links = find_tables(listing);
        assert_eq!(
            links.first().map(|link| link.date.as_str()),
            Some("2030-03-31"),
            "listing vintage is decoded"
        );
        assert_eq!(
            xlsx_url(r#"<a href="/files/table.xlsx">download</a>"#).as_deref(),
            Some("https://www.uscourts.gov/files/table.xlsx")
        );
    }

    #[test]
    #[ignore = "live U.S. Courts F-5A download"]
    fn live_latest_table_has_county_rows() {
        let table = Client::new()
            .expect("client")
            .latest()
            .expect("latest table");
        assert!(
            !table.counties.is_empty(),
            "published table contains county rows"
        );
    }
}
