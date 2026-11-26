//! Blocking client for U.S. IRS Statistics of Income public CSV datasets.
//!
//! IRS publishes these only as bulk CSV files (no query API), so the client
//! streams a file, keeps the rows of one place in memory and discards the rest:
//! the bulk file is never written to disk ([`AnswerPath::Stream`]). The small
//! per-place extract is kept in an [`EphemeralCache`] (7-day TTL, 8 MiB cap,
//! oldest evicted first) that answers only when the stream fails
//! ([`AnswerPath::Cache`]). Supply its directory with [`Client::with_cache_dir`].

use csv::StringRecord;
use ephemeral_cache::{EphemeralCache, Policy, MAX_EXTRACT_TTL};
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::thread;
use std::time::Duration;
use thiserror::Error;

/// Current IRS SOI ZIP individual income dataset.
pub const ZIP_URL: &str = "https://www.irs.gov/pub/irs-soi/23zpallagi.csv";
/// Current IRS SOI county inflow dataset.
pub const COUNTY_IN_URL: &str = "https://www.irs.gov/pub/irs-soi/countyinflow2324.csv";
/// Current IRS SOI county outflow dataset.
pub const COUNTY_OUT_URL: &str = "https://www.irs.gov/pub/irs-soi/countyoutflow2324.csv";

pub use ephemeral_cache::AnswerPath;

/// Time to live and size cap of the per-place extract cache.
pub const CACHE_POLICY: Policy = Policy {
    ttl: MAX_EXTRACT_TTL,
    cap_bytes: 8 * 1024 * 1024,
};

/// Client errors from fetching, caching, or decoding IRS data.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// A network request or response failed.
    #[error("IRS request failed: {0}")]
    Request(String),
    /// Cache file access failed.
    #[error("IRS cache failed: {0}")]
    Cache(String),
    /// CSV data could not be decoded or did not have expected columns.
    #[error("IRS CSV is invalid: {0}")]
    Csv(String),
    /// No cached copy was available while offline mode was enabled.
    #[error("IRS data unavailable offline: {0}")]
    Offline(String),
}

/// Reusable blocking IRS SOI data client.
///
/// Extracts are memoized in memory for the lifetime of the client. A supplied
/// cache directory holds them between clients as the last-resort fallback.
#[derive(Debug)]
pub struct Client {
    cache: Option<EphemeralCache>,
    offline: bool,
    extracts: HashMap<String, (Vec<StringRecord>, AnswerPath)>,
}

impl Client {
    /// Creates a client without a disk cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: None,
            offline: false,
            extracts: HashMap::new(),
        }
    }

    /// Creates a client whose per-place extracts are cached under the directory.
    #[must_use]
    pub fn with_cache_dir(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            cache: Some(EphemeralCache::with_policy(path, CACHE_POLICY)),
            ..Self::new()
        }
    }

    /// Prevents network access; only a fresh cached extract can answer.
    #[must_use]
    pub fn offline(mut self) -> Self {
        self.offline = true;
        self
    }

    /// Returns a ZIP income metric for a five-digit ZIP code, with the path
    /// that answered.
    ///
    /// # Errors
    ///
    /// Returns an error if the stream and the cache both fail, or the CSV is invalid.
    pub fn zip_metric(
        &mut self,
        zip: &str,
        metric: &str,
    ) -> Result<Option<(f64, AnswerPath)>, Error> {
        let (rows, path) = self.extract(&format!("zip:{zip}"), ZIP_URL, &|_, row| {
            row.get(2) == Some(zip)
        })?;
        Ok(zip_value(&rows, zip, metric)?.map(|value| (value, path)))
    }

    /// Returns an inflow, outflow, or net migration metric for a county or
    /// state, with the path that answered.
    ///
    /// # Errors
    ///
    /// Returns an error if either stream and the cache both fail, or the CSV is invalid.
    pub fn migration_metric(
        &mut self,
        level: &str,
        geo_id: &str,
        metric: &str,
    ) -> Result<Option<(f64, AnswerPath)>, Error> {
        let key = format!("{level}:{geo_id}");
        let (inflow, in_path) = self.extract(&format!("in:{key}"), COUNTY_IN_URL, &|h, row| {
            migration_row(h, row, true, level, geo_id)
        })?;
        let (outflow, out_path) =
            self.extract(&format!("out:{key}"), COUNTY_OUT_URL, &|h, row| {
                migration_row(h, row, false, level, geo_id)
            })?;
        let path = if in_path == AnswerPath::Cache || out_path == AnswerPath::Cache {
            AnswerPath::Cache
        } else {
            AnswerPath::Stream
        };
        let (incoming, outgoing) = migration_values(&inflow, &outflow, level, geo_id)?;
        let value = match metric {
            "in_migration_returns" => incoming.0,
            "out_migration_returns" => outgoing.0,
            "net_migration_returns" => incoming.0 - outgoing.0,
            "in_migration_agi" => incoming.1,
            "out_migration_agi" => outgoing.1,
            "net_migration_agi" => incoming.1 - outgoing.1,
            _ => return Ok(None),
        };
        Ok(Some((value, path)))
    }

    /// Header plus the rows `keep` accepts, from the stream or else the cache.
    fn extract(
        &mut self,
        key: &str,
        url: &str,
        keep: &dyn Fn(&StringRecord, &StringRecord) -> bool,
    ) -> Result<(Vec<StringRecord>, AnswerPath), Error> {
        if let Some(hit) = self.extracts.get(key) {
            return Ok(hit.clone());
        }
        let offline = self.offline;
        let live = || {
            if offline {
                return Err(Error::Offline(format!("not streaming {url}")));
            }
            stream_extract(url, keep)
        };
        let (bytes, path) = match &self.cache {
            Some(cache) => cache.resolve(key, AnswerPath::Stream, live)?,
            None => (live()?, AnswerPath::Stream),
        };
        let rows = parse_rows(&bytes, url)?;
        self.extracts.insert(key.to_owned(), (rows.clone(), path));
        Ok((rows, path))
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

/// Streams `url` and returns the CSV of its header plus the rows `keep`
/// accepts. Nothing is written to disk; transient failures are retried.
fn stream_extract(
    url: &str,
    keep: &dyn Fn(&StringRecord, &StringRecord) -> bool,
) -> Result<Vec<u8>, Error> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("statsapi-irs-soi/0.1 (public data client)")
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|error| Error::Request(error.to_string()))?;
    let mut last_error = String::new();
    for attempt in 0..4 {
        let response = client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status);
        match response {
            Ok(response) => match filter_rows(response, keep, url) {
                Err(Error::Request(message)) => last_error = message,
                done => return done,
            },
            Err(error) => {
                let retryable = error.is_timeout()
                    || error.is_connect()
                    || error.is_body()
                    || error
                        .status()
                        .is_some_and(|status| status.as_u16() == 429 || status.is_server_error());
                last_error = error.to_string();
                if !retryable {
                    break;
                }
            }
        }
        if attempt < 3 {
            let base_ms = 200_u64.saturating_mul(1_u64 << attempt);
            thread::sleep(Duration::from_millis(
                base_ms + u64::from(fastrand::u8(..40)),
            ));
        }
    }
    Err(Error::Request(format!("fetching {url}: {last_error}")))
}

/// Reads CSV from `source` row by row, keeping the header and accepted rows.
/// A read failure is [`Error::Request`] (retryable); bad CSV is [`Error::Csv`].
fn filter_rows(
    source: impl Read,
    keep: &dyn Fn(&StringRecord, &StringRecord) -> bool,
    url: &str,
) -> Result<Vec<u8>, Error> {
    let mut reader = csv::ReaderBuilder::new().flexible(true).from_reader(source);
    let headers = reader
        .headers()
        .map_err(|error| Error::Request(format!("reading header {url}: {error}")))?
        .clone();
    let mut writer = csv::Writer::from_writer(Vec::new());
    let csv_error = |error: csv::Error| Error::Csv(format!("writing extract of {url}: {error}"));
    writer.write_record(&headers).map_err(csv_error)?;
    let mut record = StringRecord::new();
    while reader
        .read_record(&mut record)
        .map_err(|error| Error::Request(format!("streaming {url}: {error}")))?
    {
        if keep(&headers, &record) {
            writer.write_record(&record).map_err(csv_error)?;
        }
    }
    writer
        .into_inner()
        .map_err(|error| Error::Csv(format!("finishing extract of {url}: {error}")))
}

/// Whether a migration row is the all-destinations total of the requested place.
fn migration_row(
    headers: &StringRecord,
    row: &StringRecord,
    incoming: bool,
    level: &str,
    id: &str,
) -> bool {
    let (state_col, county_col, label_col) = if incoming {
        ("y2_statefips", "y2_countyfips", "y1_countyname")
    } else {
        ("y1_statefips", "y1_countyfips", "y2_countyname")
    };
    let at = |name: &str| {
        headers
            .iter()
            .position(|field| field == name)
            .and_then(|index| row.get(index))
            .unwrap_or("")
    };
    at(label_col).ends_with("Total Migration-US and Foreign")
        && match level {
            "county" => format!("{}{}", at(state_col), at(county_col)) == id,
            "state" => at(state_col) == id,
            _ => false,
        }
}

fn parse_rows(bytes: &[u8], url: &str) -> Result<Vec<StringRecord>, Error> {
    let mut reader = csv::ReaderBuilder::new().flexible(true).from_reader(bytes);
    let mut rows = vec![reader
        .headers()
        .map_err(|error| Error::Csv(format!("reading header {url}: {error}")))?
        .clone()];
    rows.extend(
        reader
            .records()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| Error::Csv(format!("parsing {url}: {error}")))?,
    );
    Ok(rows)
}

/// Parses and computes a ZIP metric from a CSV document, useful for offline workflows.
///
/// # Errors
///
/// Returns an error when the CSV is malformed, required columns are absent, or a value is invalid.
pub fn parse_zip_metric(csv: &str, zip: &str, metric: &str) -> Result<Option<f64>, Error> {
    zip_value(&parse_rows(csv.as_bytes(), "fixture")?, zip, metric)
}

/// Parses and computes a migration metric from two CSV documents, useful offline.
///
/// # Errors
///
/// Returns an error when either CSV is malformed, required columns are absent, or a value is invalid.
pub fn parse_migration_metric(
    inflow_csv: &str,
    outflow_csv: &str,
    level: &str,
    id: &str,
    metric: &str,
) -> Result<Option<f64>, Error> {
    let incoming = parse_rows(inflow_csv.as_bytes(), "inflow fixture")?;
    let outgoing = parse_rows(outflow_csv.as_bytes(), "outflow fixture")?;
    let (incoming, outgoing) = migration_values(&incoming, &outgoing, level, id)?;
    Ok(match metric {
        "in_migration_returns" => Some(incoming.0),
        "out_migration_returns" => Some(outgoing.0),
        "net_migration_returns" => Some(incoming.0 - outgoing.0),
        "in_migration_agi" => Some(incoming.1),
        "out_migration_agi" => Some(outgoing.1),
        "net_migration_agi" => Some(incoming.1 - outgoing.1),
        _ => None,
    })
}

fn number(record: &StringRecord, headers: &StringRecord, name: &str) -> Result<f64, Error> {
    let index = headers
        .iter()
        .position(|header| header.eq_ignore_ascii_case(name))
        .ok_or_else(|| Error::Csv(format!("missing {name} column")))?;
    let raw = record.get(index).unwrap_or("").trim();
    if raw.is_empty() || raw == "-" {
        return Ok(0.0);
    }
    raw.parse::<f64>()
        .map_err(|error| Error::Csv(format!("invalid {name} value {raw:?}: {error}")))
}

fn zip_value(rows: &[StringRecord], zip: &str, metric: &str) -> Result<Option<f64>, Error> {
    let Some(headers) = rows.first() else {
        return Ok(None);
    };
    let mut totals = BTreeMap::<String, f64>::new();
    let mut high_returns = 0.0;
    for row in rows
        .iter()
        .filter(|row| row.get(2).is_some_and(|value| value == zip))
    {
        for column in [
            "N1", "A00100", "A00200", "A00600", "A01000", "N59660", "N04470",
        ] {
            *totals.entry(column.to_owned()).or_default() += number(row, headers, column)?;
        }
        if row.get(3) == Some("6") {
            high_returns += number(row, headers, "N1")?;
        }
    }
    let get = |name: &str| totals.get(name).copied().unwrap_or_default();
    let returns = get("N1");
    if returns == 0.0 {
        return Ok(None);
    }
    Ok(match metric {
        "returns" => Some(returns),
        "agi" => Some(get("A00100")),
        "mean_agi" => Some(get("A00100") / returns),
        "wages" => Some(get("A00200")),
        "dividends" => Some(get("A00600")),
        "capital_gains" => Some(get("A01000")),
        "eitc_share" => Some(get("N59660") / returns),
        "agi_200k_share" => Some(high_returns / returns),
        "itemizers_share" => Some(get("N04470") / returns),
        _ => None,
    })
}

type CountAndAgi = (f64, f64);
type MigrationFlows = (CountAndAgi, CountAndAgi);

fn migration_values(
    inflow: &[StringRecord],
    outflow: &[StringRecord],
    level: &str,
    id: &str,
) -> Result<MigrationFlows, Error> {
    let parse = |rows: &[StringRecord], incoming: bool| -> Result<(f64, f64), Error> {
        let Some(headers) = rows.first() else {
            return Ok((0.0, 0.0));
        };
        let (state_col, county_col, label_col) = if incoming {
            ("y2_statefips", "y2_countyfips", "y1_countyname")
        } else {
            ("y1_statefips", "y1_countyfips", "y2_countyname")
        };
        let mut returns = 0.0;
        let mut agi = 0.0;
        for row in rows {
            let column = headers
                .iter()
                .position(|field| field == label_col)
                .unwrap_or(0);
            if !row
                .get(column)
                .unwrap_or("")
                .ends_with("Total Migration-US and Foreign")
            {
                continue;
            }
            let state = headers
                .iter()
                .position(|field| field == state_col)
                .and_then(|index| row.get(index))
                .unwrap_or("");
            let county = headers
                .iter()
                .position(|field| field == county_col)
                .and_then(|index| row.get(index))
                .unwrap_or("");
            let matches = match level {
                "county" => format!("{state}{county}") == id,
                "state" => state == id,
                _ => false,
            };
            if matches {
                returns += number(row, headers, "n1")?;
                agi += number(row, headers, "agi")?;
            }
        }
        Ok((returns, agi))
    };
    Ok((parse(inflow, true)?, parse(outflow, false)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_fixtures_are_synthetic_and_metrics_decode() {
        let zip = include_str!("../tests/fixtures/zip.csv");
        assert_eq!(
            parse_zip_metric(zip, "12345", "returns").unwrap(),
            Some(15.0)
        );
        assert_eq!(
            parse_zip_metric(zip, "12345", "mean_agi").unwrap(),
            Some(200.0)
        );
        assert_eq!(
            parse_zip_metric(zip, "12345", "agi_200k_share").unwrap(),
            Some(1.0 / 3.0)
        );
    }

    #[test]
    fn migration_fixture_decodes_in_out_and_net() {
        let inflow = include_str!("../tests/fixtures/county-in.csv");
        let outflow = include_str!("../tests/fixtures/county-out.csv");
        assert_eq!(
            parse_migration_metric(inflow, outflow, "county", "99001", "net_migration_returns")
                .unwrap(),
            Some(4.0)
        );
        assert_eq!(
            parse_migration_metric(inflow, outflow, "county", "99001", "net_migration_agi")
                .unwrap(),
            Some(450.0)
        );
    }

    fn seeded(dir: &std::path::Path, key: &str, csv: &str) -> Client {
        EphemeralCache::with_policy(dir, CACHE_POLICY)
            .put(key, csv.as_bytes())
            .unwrap();
        Client::with_cache_dir(dir).offline()
    }

    #[test]
    fn offline_client_without_a_cached_extract_reports_a_miss() {
        let dir = tempfile::tempdir().unwrap();
        let result = Client::with_cache_dir(dir.path())
            .offline()
            .zip_metric("12345", "returns");
        assert!(matches!(result, Err(Error::Offline(_))));
    }

    #[test]
    fn a_fresh_cached_extract_answers_when_the_stream_is_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let zip = include_str!("../tests/fixtures/zip.csv");
        let mut client = seeded(dir.path(), "zip:12345", zip);
        assert_eq!(
            client.zip_metric("12345", "returns").unwrap(),
            Some((15.0, AnswerPath::Cache))
        );
    }

    #[test]
    fn an_expired_cached_extract_is_not_served() {
        let dir = tempfile::tempdir().unwrap();
        let zip = include_str!("../tests/fixtures/zip.csv");
        let mut client = seeded(dir.path(), "zip:12345", zip);
        for entry in std::fs::read_dir(dir.path()).unwrap() {
            let file = std::fs::File::options()
                .write(true)
                .open(entry.unwrap().path())
                .unwrap();
            file.set_modified(
                std::time::SystemTime::now() - MAX_EXTRACT_TTL - Duration::from_secs(60),
            )
            .unwrap();
        }
        assert!(matches!(
            client.zip_metric("12345", "returns"),
            Err(Error::Offline(_))
        ));
    }

    #[test]
    fn streaming_keeps_only_the_requested_rows_and_stores_no_archive() {
        let zip = include_str!("../tests/fixtures/zip.csv");
        let kept = filter_rows(zip.as_bytes(), &|_, row| row.get(2) == Some("12345"), "t").unwrap();
        let rows = parse_rows(&kept, "t").unwrap();
        assert!(rows.iter().skip(1).all(|row| row.get(2) == Some("12345")));
        let dir = tempfile::tempdir().unwrap();
        let mut client = Client::with_cache_dir(dir.path());
        client
            .extracts
            .insert("zip:12345".into(), (rows, AnswerPath::Stream));
        assert_eq!(
            client.zip_metric("12345", "returns").unwrap(),
            Some((15.0, AnswerPath::Stream))
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
