//! Blocking readers and parsers for IRS exempt-organization bulk files.

use csv::StringRecord;
use std::io::{BufRead, Cursor, Read};
use std::time::Duration;
use thiserror::Error;

/// IRS EO Business Master File root.
pub const BMF_BASE: &str = "https://www.irs.gov/pub/irs-soi";
/// Publication 78 ZIP endpoint.
pub const PUB78_URL: &str = "https://apps.irs.gov/pub/epostcard/data-download-pub78.zip";
/// Automatic revocation ZIP endpoint.
pub const REVOCATION_URL: &str = "https://apps.irs.gov/pub/epostcard/data-download-revocation.zip";

/// Parsed BMF fields needed by nonprofit adapters.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Organization {
    pub ein: Option<u64>,
    pub name: Option<String>,
    pub address: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub zip: Option<String>,
    pub subsection: Option<u16>,
    pub foundation: Option<u16>,
    pub deductibility: Option<u16>,
    pub exempt_status: Option<u16>,
    pub organization: Option<u16>,
    pub affiliation: Option<u16>,
    pub filing_requirement: Option<u16>,
    pub ntee: Option<String>,
    pub ruling: Option<String>,
    pub tax_period: Option<String>,
    pub asset_amount: Option<i64>,
    pub income_amount: Option<i64>,
    pub revenue_amount: Option<i64>,
}

/// Errors returned by IRS file access or parsing.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// Network or HTTP failure.
    #[error("IRS request failed: {0}")]
    Request(String),
    /// CSV, ZIP, or fixed-width content was invalid.
    #[error("IRS file is invalid: {0}")]
    Decode(String),
}

/// A blocking client with no environment or cache policy.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
}

impl Client {
    /// Creates a client with a bounded request timeout.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be initialized.
    pub fn new() -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(600))
            .user_agent("statsapi-irs-eo/0.1 (+https://github.com/davidawad/statsapi)")
            .build()
            .map_err(|e| Error::Request(e.to_string()))?;
        Ok(Self { http })
    }
    /// Downloads one IRS resource into memory.
    ///
    /// # Errors
    ///
    /// Returns an error when the request fails or the server rejects it.
    pub fn download(&self, url: &str) -> Result<Vec<u8>, Error> {
        self.http
            .get(url)
            .send()
            .map_err(|e| Error::Request(e.to_string()))?
            .error_for_status()
            .map_err(|e| Error::Request(e.to_string()))?
            .bytes()
            .map(|b| b.to_vec())
            .map_err(|e| Error::Request(e.to_string()))
    }
}

/// Parses the first BMF row matching an EIN or normalized name.
///
/// # Errors
///
/// Returns an error when the bytes are not a valid CSV table.
pub fn parse_bmf(
    bytes: &[u8],
    ein: Option<u64>,
    name: Option<&str>,
) -> Result<Option<Organization>, Error> {
    let mut reader = csv::ReaderBuilder::new().flexible(true).from_reader(bytes);
    let header = reader
        .headers()
        .map_err(|e| Error::Decode(e.to_string()))?
        .clone();
    for row in reader.records() {
        let row = row.map_err(|e| Error::Decode(e.to_string()))?;
        let matches_ein = ein.is_some_and(|wanted| {
            field(&header, &row, "EIN").and_then(|v| v.parse().ok()) == Some(wanted)
        });
        let matches_name = name.is_some_and(|wanted| {
            field(&header, &row, "NAME").is_some_and(|value| normalize(value) == normalize(wanted))
        });
        if matches_ein || matches_name {
            return Ok(Some(from_record(&header, &row)));
        }
    }
    Ok(None)
}

/// Filters a BMF stream and returns a header-preserving CSV extract.
///
/// # Errors
///
/// Returns an error when the input or output CSV cannot be parsed or written.
pub fn extract_bmf(
    reader: impl Read,
    keep: impl Fn(&StringRecord, &StringRecord) -> bool,
) -> Result<Vec<u8>, Error> {
    let mut csv = csv::ReaderBuilder::new().flexible(true).from_reader(reader);
    let header = csv
        .headers()
        .map_err(|e| Error::Decode(e.to_string()))?
        .clone();
    let mut output = csv::Writer::from_writer(Vec::new());
    output
        .write_record(&header)
        .map_err(|e| Error::Decode(e.to_string()))?;
    for row in csv.records() {
        let row = row.map_err(|e| Error::Decode(e.to_string()))?;
        if keep(&header, &row) {
            output
                .write_record(&row)
                .map_err(|e| Error::Decode(e.to_string()))?;
        }
    }
    output
        .into_inner()
        .map_err(|e| Error::Decode(e.to_string()))
}

/// Reads a header-preserving BMF extract.
///
/// # Errors
///
/// Returns an error when the bytes are not a valid CSV table.
pub fn records(bytes: &[u8]) -> Result<(StringRecord, Vec<StringRecord>), Error> {
    let mut csv = csv::ReaderBuilder::new().flexible(true).from_reader(bytes);
    let header = csv
        .headers()
        .map_err(|e| Error::Decode(e.to_string()))?
        .clone();
    let rows = csv
        .records()
        .map(|row| row.map_err(|e| Error::Decode(e.to_string())))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((header, rows))
}

/// Finds an EIN line in a pipe-delimited IRS file.
///
/// # Errors
///
/// Returns an error when the bytes cannot be read as a line-oriented file.
pub fn find_pipe_line(bytes: &[u8], ein: u64) -> Result<Option<String>, Error> {
    let want = format!("{ein:09}|");
    for line in bytes.lines() {
        let line = line.map_err(|e| Error::Decode(e.to_string()))?;
        if line.starts_with(&want) {
            return Ok(Some(line));
        }
    }
    Ok(None)
}

/// Unpacks the first member of an IRS ZIP response.
///
/// # Errors
///
/// Returns an error when the bytes are not a valid ZIP archive or its first entry cannot be read.
pub fn unzip_first(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| Error::Decode(e.to_string()))?;
    let mut file = archive
        .by_index(0)
        .map_err(|e| Error::Decode(e.to_string()))?;
    let mut out = Vec::new();
    file.read_to_end(&mut out)
        .map_err(|e| Error::Decode(e.to_string()))?;
    Ok(out)
}

fn from_record(header: &StringRecord, row: &StringRecord) -> Organization {
    let text = |name| {
        field(header, row, name)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
    };
    Organization {
        ein: parse(header, row, "EIN"),
        name: text("NAME"),
        address: text("STREET"),
        city: text("CITY"),
        state: text("STATE"),
        zip: text("ZIP"),
        subsection: parse(header, row, "SUBSECTION"),
        foundation: parse(header, row, "FOUNDATION"),
        deductibility: parse(header, row, "DEDUCTIBILITY"),
        exempt_status: parse(header, row, "STATUS"),
        organization: parse(header, row, "ORGANIZATION"),
        affiliation: parse(header, row, "AFFILIATION"),
        filing_requirement: parse(header, row, "FILING_REQ_CD"),
        ntee: text("NTEE_CD"),
        ruling: yyyymm(field(header, row, "RULING")),
        tax_period: yyyymm(field(header, row, "TAX_PERIOD")),
        asset_amount: parse(header, row, "ASSET_AMT"),
        income_amount: parse(header, row, "INCOME_AMT"),
        revenue_amount: parse(header, row, "REVENUE_AMT"),
    }
}

fn field<'a>(header: &StringRecord, row: &'a StringRecord, name: &str) -> Option<&'a str> {
    header
        .iter()
        .position(|column| column.eq_ignore_ascii_case(name))
        .and_then(|i| row.get(i))
        .map(str::trim)
}
fn parse<T: std::str::FromStr>(header: &StringRecord, row: &StringRecord, name: &str) -> Option<T> {
    field(header, row, name)
        .filter(|v| !v.is_empty())
        .and_then(|v| v.parse().ok())
}
fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase()
}
fn yyyymm(value: Option<&str>) -> Option<String> {
    value
        .filter(|s| s.len() == 6 && s.chars().all(|c| c.is_ascii_digit()) && *s != "000000")
        .map(|s| format!("{}-{}", &s[..4], &s[4..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_synthetic_bmf_row() {
        let csv = b"EIN,NAME,STATE,RULING\n123456789,Example Org,NJ,202401\n";
        assert_eq!(
            parse_bmf(csv, Some(123_456_789), None)
                .expect("fixture")
                .expect("row")
                .ruling
                .as_deref(),
            Some("2024-01")
        );
    }
}
