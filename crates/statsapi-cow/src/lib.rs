//! Small, reusable decoder for the public COW CSV data releases.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::time::Duration;
use thiserror::Error;
use zip::ZipArchive;

const MAX_DOWNLOAD: usize = 16 * 1024 * 1024;

/// A decoded CSV row keyed by its published column names.
pub type Row = BTreeMap<String, String>;

/// Errors returned while decoding a COW release.
#[derive(Debug, Error)]
pub enum Error {
    /// The CSV header or rows were malformed.
    #[error("invalid COW CSV: {0}")]
    Csv(#[from] csv::Error),
    /// The source server returned a failed response or could not be reached.
    #[error("COW source request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// The source archive is invalid or does not contain the requested member.
    #[error("invalid COW archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    /// A source archive could not be read or exceeded the configured limit.
    #[error("COW source I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// The source exceeded the 16 MiB download or extract limit.
    #[error("COW source exceeds the 16 MiB limit")]
    TooLarge,
    /// The requested CSV member was absent from the archive.
    #[error("COW archive member not found: {0}")]
    MemberMissing(String),
}

/// Fetches one public COW CSV file without retaining it on disk.
///
/// # Errors
///
/// Returns an error for a failed request or a response larger than 16 MiB.
pub fn fetch_csv(url: &str) -> Result<Vec<u8>, Error> {
    let bytes = fetch_bytes(url)?;
    if bytes.len() > MAX_DOWNLOAD {
        return Err(Error::TooLarge);
    }
    Ok(bytes)
}

/// Fetches and extracts one CSV member from a public COW ZIP release.
///
/// # Errors
///
/// Returns an error for a failed request, invalid archive, missing member, or
/// extracted CSV larger than 16 MiB.
pub fn fetch_archive_member(url: &str, member: &str) -> Result<Vec<u8>, Error> {
    let bytes = fetch_bytes(url)?;
    if bytes.len() > MAX_DOWNLOAD {
        return Err(Error::TooLarge);
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        if file.name()?.ends_with(member) {
            let mut output = Vec::new();
            file.take((MAX_DOWNLOAD + 1) as u64)
                .read_to_end(&mut output)?;
            if output.len() > MAX_DOWNLOAD {
                return Err(Error::TooLarge);
            }
            return Ok(output);
        }
    }
    Err(Error::MemberMissing(member.to_string()))
}

fn fetch_bytes(url: &str) -> Result<Vec<u8>, Error> {
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?
        .get(url)
        .send()?
        .error_for_status()?;
    Ok(response.bytes()?.to_vec())
}

/// Decodes a published COW CSV file.
///
/// # Errors
///
/// Returns an error when the input is not a valid CSV document.
pub fn decode_csv(bytes: &[u8]) -> Result<Vec<Row>, Error> {
    let decoded = String::from_utf8_lossy(bytes);
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(decoded.as_bytes());
    let headers = reader.headers()?.clone();
    reader
        .records()
        .map(|record| {
            let record = record?;
            Ok(headers
                .iter()
                .zip(record.iter())
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_participant_rows_by_published_header() {
        let rows = decode_csv(b"ccode,styear\n2,2001\n").expect("valid fixture");
        assert_eq!(rows[0]["ccode"], "2");
        assert_eq!(rows[0]["styear"], "2001");
    }
}
