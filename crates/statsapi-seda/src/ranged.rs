//! Keyed range reads of a release CSV that is sorted by its numeric id column.
//!
//! The SEDA releases are hundreds of megabytes sorted by geography id, so one
//! geography is found by bisecting with HTTP range requests and then streaming
//! only its rows, instead of streaming the whole file.

use crate::{Error, Table};
use csv::{ReaderBuilder, StringRecord};
use reqwest::blocking::Client as Http;
use reqwest::StatusCode;
use std::io::Read;

/// Bytes read to find the header row and the file size.
const HEAD: u64 = 64 * 1024;
/// Bytes read per bisection probe; must hold one full row.
const PROBE: u64 = 32 * 1024;
/// Stop bisecting once the open window is this small and scan it instead.
const SCAN: u64 = 256 * 1024;

fn get(http: &Http, url: &str, range: &str) -> Result<reqwest::blocking::Response, Error> {
    Ok(http.get(url).header("Range", range).send()?)
}

fn total_size(response: &reqwest::blocking::Response) -> Option<u64> {
    let range = response.headers().get("content-range")?.to_str().ok()?;
    range.rsplit('/').next()?.parse().ok()
}

fn read_up_to(response: impl Read, limit: u64) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    response.take(limit).read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn parse_line(line: &[u8]) -> Option<StringRecord> {
    ReaderBuilder::new()
        .has_headers(false)
        .from_reader(line)
        .records()
        .next()?
        .ok()
}

fn numeric_id(row: &StringRecord, index: usize) -> Option<u64> {
    row.get(index)?.trim().parse().ok()
}

/// The rows whose id column equals `code`, or `None` when the server or the file
/// layout does not allow a keyed read (the caller then streams the whole file).
pub(crate) fn fetch(
    http: &Http,
    url: &str,
    id_column: &'static str,
    code: &str,
    vintage: &'static str,
) -> Result<Option<Table>, Error> {
    let Ok(target) = code.trim().parse::<u64>() else {
        return Ok(None);
    };
    let response = get(http, url, &format!("bytes=0-{}", HEAD - 1))?;
    if response.status() != StatusCode::PARTIAL_CONTENT {
        return Ok(None);
    }
    let Some(size) = total_size(&response) else {
        return Ok(None);
    };
    let head = read_up_to(response, HEAD)?;
    let Some(header_end) = head.iter().position(|byte| *byte == b'\n') else {
        return Ok(None);
    };
    let Some(headers) = parse_line(&head[..header_end]) else {
        return Ok(None);
    };
    let Some(id_index) = headers.iter().position(|name| name == id_column) else {
        return Err(Error::MissingIdColumn(id_column));
    };
    // Every row before `low` has an id below the target; the first row with an id
    // at or past it starts before `high`'s next line.
    let (mut low, mut high) = (header_end as u64 + 1, size);
    while high.saturating_sub(low) > SCAN {
        let mid = low + (high - low) / 2;
        let probe = read_up_to(
            get(http, url, &format!("bytes={mid}-{}", mid + PROBE - 1))?,
            PROBE,
        )?;
        let Some(start) = probe.iter().position(|byte| *byte == b'\n').map(|i| i + 1) else {
            return Ok(None);
        };
        let Some(length) = probe[start..].iter().position(|byte| *byte == b'\n') else {
            return Ok(None);
        };
        let Some(id) =
            parse_line(&probe[start..start + length]).and_then(|row| numeric_id(&row, id_index))
        else {
            return Ok(None);
        };
        if id < target {
            low = mid + start as u64;
        } else {
            high = mid;
        }
    }
    let response = get(http, url, &format!("bytes={low}-"))?;
    if response.status() != StatusCode::PARTIAL_CONTENT {
        return Ok(None);
    }
    let mut reader = ReaderBuilder::new()
        .has_headers(false)
        .from_reader(response);
    let mut rows = Vec::new();
    for row in reader.records() {
        let row = row?;
        let Some(id) = numeric_id(&row, id_index) else {
            return Ok(None);
        };
        if id > target {
            break;
        }
        if row.get(id_index) == Some(code) {
            rows.push(row);
        }
    }
    Ok(Some(Table {
        headers,
        rows,
        source_url: url.to_owned(),
        vintage,
    }))
}
