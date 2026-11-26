//! Blocking client for utility rate CSVs and EIA retail price data.

#[cfg(not(feature = "cache"))]
use access::Flight;
#[cfg(not(feature = "cache"))]
pub use access::{Source, Trail};
use calamine::{open_workbook_from_rs, Reader, Xls, Xlsx};
#[cfg(feature = "cache")]
use ephemeral_cache::{EphemeralCache, Flight};
#[cfg(feature = "cache")]
pub use ephemeral_cache::{Source, Trail};
use reqwest::StatusCode;
use serde::Deserialize;
use std::fmt;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const EIA_API_ROOT: &str = "https://api.eia.gov/";
#[cfg(feature = "cache")]
const LIVE_TTL: Duration = Duration::from_secs(3600);
#[cfg(feature = "cache")]
const CACHE_CAP_BYTES: u64 = 16 * 1024 * 1024;

#[cfg(not(feature = "cache"))]
mod access {
    use std::collections::HashMap;
    use std::sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex, OnceLock, PoisonError,
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

        pub fn answered(&self) -> Option<Source> {
            Source::from_code(self.0.load(Ordering::Relaxed))
        }
    }

    #[derive(Debug)]
    pub struct Flight<V> {
        cells: Mutex<HashMap<String, Arc<OnceLock<V>>>>,
    }

    impl<V> Default for Flight<V> {
        fn default() -> Self {
            Self {
                cells: Mutex::new(HashMap::new()),
            }
        }
    }

    impl<V: Clone> Flight<V> {
        pub fn get_or_init(&self, key: &str, compute: impl FnOnce() -> V) -> V {
            let cell = {
                let mut cells = self.cells.lock().unwrap_or_else(PoisonError::into_inner);
                Arc::clone(cells.entry(key.to_owned()).or_default())
            };
            cell.get_or_init(compute).clone()
        }
    }
}

/// Public CSV release metadata. Some years have no available release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Vintage {
    /// EIA-861 survey year represented by the release.
    pub year: u16,
    /// Portal submission identifier.
    pub submission: u32,
    /// File name for investor-owned utility rows.
    pub iou_file: &'static str,
    /// File name for other utility rows.
    pub non_iou_file: &'static str,
}

/// Known releases with downloadable annual ZIP utility mapping files.
pub const VINTAGES: &[Vintage] = &[
    Vintage {
        year: 2011,
        submission: 450,
        iou_file: "iouzipcodes2011.csv",
        non_iou_file: "noniouzipcodes2011.csv",
    },
    Vintage {
        year: 2013,
        submission: 444,
        iou_file: "iouzipcodes2013.csv",
        non_iou_file: "noniouzipcodes2013.csv",
    },
    Vintage {
        year: 2014,
        submission: 445,
        iou_file: "iouzipcodes2014.csv",
        non_iou_file: "noniouzipcodes2014.csv",
    },
    Vintage {
        year: 2015,
        submission: 446,
        iou_file: "iouzipcodes2015.csv",
        non_iou_file: "noniouzipcodes2015.csv",
    },
    Vintage {
        year: 2016,
        submission: 447,
        iou_file: "iouzipcodes2016.csv",
        non_iou_file: "noniouzipcodes2016.csv",
    },
    Vintage {
        year: 2017,
        submission: 448,
        iou_file: "iouzipcodes2017.csv",
        non_iou_file: "noniouzipcodes2017.csv",
    },
    Vintage {
        year: 2018,
        submission: 449,
        iou_file: "iouzipcodes2018.csv",
        non_iou_file: "noniouzipcodes2018.csv",
    },
    Vintage {
        year: 2019,
        submission: 4042,
        iou_file: "iou_zipcodes_2019.csv",
        non_iou_file: "non_iou_zipcodes_2019.csv",
    },
    Vintage {
        year: 2020,
        submission: 5650,
        iou_file: "iou_zipcodes_2020.csv",
        non_iou_file: "non_iou_zipcodes_2020.csv",
    },
    Vintage {
        year: 2021,
        submission: 5806,
        iou_file: "iou_zipcodes_2021.csv",
        non_iou_file: "non_iou_zipcodes_2021.csv",
    },
    Vintage {
        year: 2022,
        submission: 5993,
        iou_file: "iou_zipcodes_2022.csv",
        non_iou_file: "non_iou_zipcodes_2022.csv",
    },
    Vintage {
        year: 2023,
        submission: 6225,
        iou_file: "iou_zipcodes_2023.csv",
        non_iou_file: "non_iou_zipcodes_2023.csv",
    },
    Vintage {
        year: 2024,
        submission: 8563,
        iou_file: "iou_zipcodes_2024.csv",
        non_iou_file: "non_iou_zipcodes_2024.csv",
    },
    Vintage {
        year: 2025,
        submission: 10259,
        iou_file: "iou_zipcodes_2025.csv",
        non_iou_file: "non_iou_zipcodes_2025.csv",
    },
];

/// One utility's annual average rates associated with a ZIP code.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct UtilityRate {
    /// Five-digit ZIP code.
    #[serde(rename = "zip")]
    pub zip_code: String,
    /// EIA utility identifier.
    #[serde(rename = "eiaid")]
    pub utility_id: String,
    /// Published utility name.
    pub utility_name: String,
    /// State abbreviation.
    pub state: String,
    /// Bundled, delivery, or other service type.
    pub service_type: String,
    /// Utility ownership classification.
    pub ownership: String,
    /// Commercial dollars per kWh.
    pub comm_rate: f64,
    /// Industrial dollars per kWh.
    pub ind_rate: f64,
    /// Residential dollars per kWh.
    pub res_rate: f64,
}

/// EIA annual utility price calculated from revenue and sales.
#[derive(Clone, Debug, PartialEq)]
pub struct AnnualUtilityRate {
    /// Reporting year.
    pub year: u16,
    /// EIA utility identifier.
    pub utility_id: String,
    /// Utility name.
    pub utility_name: String,
    /// Reporting state.
    pub state: String,
    /// Bundled or delivery service type.
    pub service_type: String,
    /// Ownership classification.
    pub ownership: String,
    /// Residential price in dollars per kWh.
    pub residential: Option<f64>,
    /// Commercial price in dollars per kWh.
    pub commercial: Option<f64>,
    /// Industrial price in dollars per kWh.
    pub industrial: Option<f64>,
}

/// Client errors from download, cache, JSON, or CSV handling.
#[derive(Clone, Debug)]
pub enum Error {
    /// A request or filesystem operation failed.
    Request(String),
    /// A response could not be decoded.
    Decode(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(message) => write!(formatter, "request failed: {message}"),
            Self::Decode(message) => write!(formatter, "response decode failed: {message}"),
        }
    }
}
impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Request(error.to_string())
    }
}

/// Blocking data client. Bulk files are streamed and never kept; EIA API
/// responses may be kept in an optional last-resort cache (1 h TTL, 64 MiB cap).
#[derive(Clone, Debug, Default)]
pub struct Client {
    cache_dir: Option<PathBuf>,
    trail: Trail,
    once: Arc<Once>,
}

/// An answer shared by every definition of one run, with the path that produced it.
type Shared<T> = Result<(Arc<T>, Source), Error>;

/// Per-run request dedupe: each URL is downloaded, and each release decoded, once.
#[derive(Debug, Default)]
struct Once {
    bytes: Flight<Shared<[u8]>>,
    zip: Flight<Shared<[UtilityRate]>>,
    annual: Flight<Shared<[AnnualUtilityRate]>>,
}

impl Client {
    /// Construct a client without a persistent cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The access paths (api|stream|cache) this client has answered with.
    #[must_use]
    pub fn trail(&self) -> Trail {
        self.trail.clone()
    }

    /// A handle with its own access-path trail that shares this client's request
    /// dedupe, for one definition among concurrent ones.
    #[must_use]
    pub fn fork(&self) -> Self {
        Self {
            trail: Trail::new(),
            ..self.clone()
        }
    }

    /// Note the path behind a shared answer on this handle's trail and unwrap it.
    fn noted<T: ?Sized>(&self, shared: Shared<T>) -> Result<Arc<T>, Error> {
        let (value, source) = shared?;
        self.trail.note(source);
        Ok(value)
    }

    /// Configure an optional ephemeral cache directory for EIA API responses.
    #[must_use]
    pub fn with_cache_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Download a public utility-rate CSV release and decode its rows.
    ///
    /// # Errors
    /// Returns an error for network, cache, HTTP, or CSV failures.
    pub fn zip_rates(&self, vintage: Vintage) -> Result<Arc<[UtilityRate]>, Error> {
        let shared = self.once.zip.get_or_init(&vintage.year.to_string(), || {
            let probe = self.fork();
            let rows = probe.decode_zip_rates(vintage)?;
            probe
                .trail
                .answered()
                .map(|source| (Arc::from(rows), source))
                .ok_or_else(|| Error::Request("no answer path recorded".to_owned()))
        });
        self.noted(shared)
    }

    fn decode_zip_rates(&self, vintage: Vintage) -> Result<Vec<UtilityRate>, Error> {
        let base = format!("https://data.openei.org/files/{}/", vintage.submission);
        // The two files of a release download side by side.
        let files = thread::scope(|scope| {
            [vintage.iou_file, vintage.non_iou_file]
                .map(|file| {
                    let url = format!("{base}{file}");
                    scope.spawn(move || self.fetch_once(&url))
                })
                .map(|handle| {
                    handle.join().unwrap_or_else(|_| {
                        Err(Error::Request("download worker failed".to_owned()))
                    })
                })
        });
        let mut all = Vec::new();
        for bytes in files {
            let bytes = bytes?;
            let mut reader = csv::Reader::from_reader(&bytes[..]);
            all.extend(
                reader
                    .deserialize()
                    .collect::<Result<Vec<UtilityRate>, _>>()
                    .map_err(|e| Error::Decode(e.to_string()))?,
            );
        }
        Ok(all)
    }

    /// Fetch EIA API v2 retail price rows for a state and sector.
    ///
    /// The key is supplied by the caller and is never stored or logged.
    ///
    /// # Errors
    /// Returns an error when the request or JSON response fails.
    pub fn eia_monthly(
        &self,
        state: &str,
        sector: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<EiaRow>, Error> {
        let url = format!("https://api.eia.gov/v2/electricity/retail-sales/data/?frequency=monthly&data[0]=price&facets[stateid][]={state}&facets[sectorid][]={sector}&sort[0][column]=period&sort[0][direction]=desc&length=5000{}", api_key.map_or_else(String::new, |key| format!("&api_key={key}")));
        let bytes = self.fetch(&url)?;
        let body: EiaResponse =
            serde_json::from_slice(&bytes).map_err(|e| Error::Decode(e.to_string()))?;
        Ok(body.response.data)
    }

    /// Download EIA's public 2010-current monthly state-sector workbook.
    ///
    /// # Errors
    /// Returns an error when the workbook cannot be fetched, opened, or decoded.
    pub fn eia_monthly_public(&self, state: &str, sector: &str) -> Result<Vec<EiaRow>, Error> {
        let bytes =
            self.fetch_once("https://www.eia.gov/electricity/data/eia861m/xls/sales_revenue.xlsx")?;
        decode_eia_monthly(&bytes, state, sector)
    }

    /// Download and decode EIA annual sales-to-ultimate-customers utility data.
    ///
    /// # Errors
    /// Returns an error when the archive, workbook, or required utility data is unavailable.
    pub fn annual_utility_rates(&self, year: u16) -> Result<Arc<[AnnualUtilityRate]>, Error> {
        let shared = self.once.annual.get_or_init(&year.to_string(), || {
            let probe = self.fork();
            let rows = probe.decode_annual_utility_rates(year)?;
            probe
                .trail
                .answered()
                .map(|source| (Arc::from(rows), source))
                .ok_or_else(|| Error::Request("no answer path recorded".to_owned()))
        });
        self.noted(shared)
    }

    fn decode_annual_utility_rates(&self, year: u16) -> Result<Vec<AnnualUtilityRate>, Error> {
        let archive_name = if year == 2010 {
            "f86110".to_owned()
        } else {
            format!("f861{year}")
        };
        let url =
            format!("https://www.eia.gov/electricity/data/eia861/archive/zip/{archive_name}.zip");
        let archive_bytes = self.fetch_once(&url)?;
        let mut archive = zip::ZipArchive::new(Cursor::new(&archive_bytes[..]))
            .map_err(|error| Error::Decode(error.to_string()))?;
        if year == 2010 {
            let mut workbook_bytes = Vec::new();
            archive
                .by_name("file2_2010.xls")
                .map_err(|error| Error::Decode(error.to_string()))?
                .read_to_end(&mut workbook_bytes)
                .map_err(|error| Error::Decode(error.to_string()))?;
            return decode_eia_annual_2010(&workbook_bytes);
        }
        let workbook_name = format!("Sales_Ult_Cust_{year}.xlsx");
        let mut workbook_bytes = Vec::new();
        archive
            .by_name(&workbook_name)
            .map_err(|error| Error::Decode(error.to_string()))?
            .read_to_end(&mut workbook_bytes)
            .map_err(|error| Error::Decode(error.to_string()))?;
        let mut workbook: Xlsx<_> =
            open_workbook_from_rs::<Xlsx<_>, _>(Cursor::new(workbook_bytes))
                .map_err(|error| Error::Decode(error.to_string()))?;
        let range = workbook
            .worksheet_range_at(0)
            .ok_or_else(|| Error::Decode("EIA annual workbook has no first sheet".to_owned()))?
            .map_err(|error| Error::Decode(error.to_string()))?;
        let has_ba_code = range
            .rows()
            .nth(2)
            .and_then(|row| row.get(8))
            .is_some_and(|cell| cell.to_string().eq_ignore_ascii_case("BA Code"));
        let residential_column = if has_ba_code { 9 } else { 8 };
        let rows: Vec<_> = range
            .rows()
            .skip(3)
            .filter_map(|row| {
                let year_value = row.first().map(ToString::to_string)?.parse::<u16>().ok()?;
                let utility_id = row.get(1).map(ToString::to_string)?;
                let utility_name = row.get(2).map(ToString::to_string)?;
                let service_type = row.get(4).map(ToString::to_string)?;
                let state = row.get(6).map(ToString::to_string)?;
                let ownership = row.get(7).map(ToString::to_string)?;
                Some(AnnualUtilityRate {
                    year: year_value,
                    utility_id,
                    utility_name,
                    state,
                    service_type,
                    ownership,
                    residential: price_from_cells(
                        row.get(residential_column),
                        row.get(residential_column + 1),
                    ),
                    commercial: price_from_cells(
                        row.get(residential_column + 3),
                        row.get(residential_column + 4),
                    ),
                    industrial: price_from_cells(
                        row.get(residential_column + 6),
                        row.get(residential_column + 7),
                    ),
                })
            })
            .collect();
        if rows.is_empty() {
            return Err(Error::Decode(format!(
                "EIA annual workbook has no utility rows for {year}"
            )));
        }
        Ok(rows)
    }

    /// `fetch`, once per URL per run; concurrent callers share the one download.
    fn fetch_once(&self, url: &str) -> Result<Arc<[u8]>, Error> {
        let shared = self.once.bytes.get_or_init(url, || {
            let probe = self.fork();
            let bytes = probe.fetch(url)?;
            probe
                .trail
                .answered()
                .map(|source| (Arc::from(bytes), source))
                .ok_or_else(|| Error::Request("no answer path recorded".to_owned()))
        });
        self.noted(shared)
    }

    fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        let offline = std::env::var("POWER_PRICE_DATA_OFFLINE").as_deref() == Ok("1");
        #[cfg(feature = "cache")]
        // Only the EIA JSON API is cached (last resort); the OpenEI CSV and
        // EIA workbooks are bulk files, streamed into memory and never kept.
        let cache = self
            .cache_dir
            .as_ref()
            .filter(|_| url.starts_with(EIA_API_ROOT))
            .map(|dir| EphemeralCache::new(dir, LIVE_TTL, CACHE_CAP_BYTES));
        #[cfg(feature = "cache")]
        let (bytes, source) = match &cache {
            Some(cache) => cache.answer(url, offline, || Self::download(url))?,
            None if offline => {
                return Err(Error::Request(
                    "offline: bulk sources are streamed, never cached".to_owned(),
                ))
            }
            None if url.starts_with(EIA_API_ROOT) => (Self::download(url)?, Source::Api),
            None => (Self::download(url)?, Source::Stream),
        };
        #[cfg(not(feature = "cache"))]
        let (bytes, source) = {
            if offline {
                return Err(Error::Request(
                    "offline: cache support is disabled; bulk sources are streamed".to_owned(),
                ));
            }
            let source = if url.starts_with(EIA_API_ROOT) {
                Source::Api
            } else {
                Source::Stream
            };
            (Self::download(url)?, source)
        };
        self.trail.note(source);
        Ok(bytes)
    }

    fn download(url: &str) -> Result<Vec<u8>, Error> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("statsapi-power-price/0.1")
            .build()
            .map_err(|e| Error::Request(e.to_string()))?;
        let mut last_error = String::new();
        for attempt in 0..4 {
            match client.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    let bytes = response
                        .bytes()
                        .map_err(|e| Error::Request(e.to_string()))?
                        .to_vec();
                    return Ok(bytes);
                }
                Ok(response) => {
                    let status = response.status();
                    last_error = format!("HTTP {status}");
                    if status != StatusCode::TOO_MANY_REQUESTS && !status.is_server_error() {
                        break;
                    }
                }
                Err(error) => last_error = error.to_string(),
            }
            if attempt < 3 {
                retry_delay(attempt);
            }
        }
        Err(Error::Request(last_error))
    }
}

fn decode_eia_annual_2010(bytes: &[u8]) -> Result<Vec<AnnualUtilityRate>, Error> {
    let mut workbook: Xls<_> = open_workbook_from_rs::<Xls<_>, _>(Cursor::new(bytes))
        .map_err(|error| Error::Decode(error.to_string()))?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or_else(|| Error::Decode("EIA 2010 annual workbook has no first sheet".to_owned()))?
        .map_err(|error| Error::Decode(error.to_string()))?;
    let rows: Vec<_> = range
        .rows()
        .skip(3)
        .filter_map(|row| {
            let year = row.first()?.to_string().parse::<u16>().ok()?;
            Some(AnnualUtilityRate {
                year,
                utility_id: row.get(1)?.to_string(),
                utility_name: row.get(2)?.to_string(),
                service_type: row.get(4)?.to_string(),
                state: row.get(6)?.to_string(),
                ownership: row.get(7)?.to_string(),
                residential: price_from_cells(row.get(8), row.get(9)),
                commercial: price_from_cells(row.get(11), row.get(12)),
                industrial: price_from_cells(row.get(14), row.get(15)),
            })
        })
        .collect();
    if rows.is_empty() {
        return Err(Error::Decode(
            "EIA 2010 annual workbook has no utility rows".to_owned(),
        ));
    }
    Ok(rows)
}

fn price_from_cells(
    revenue: Option<&calamine::Data>,
    sales: Option<&calamine::Data>,
) -> Option<f64> {
    let revenue = revenue?.to_string().parse::<f64>().ok()?;
    let sales = sales?.to_string().parse::<f64>().ok()?;
    (sales > 0.0).then_some(revenue / sales)
}

fn decode_eia_monthly(bytes: &[u8], state: &str, sector: &str) -> Result<Vec<EiaRow>, Error> {
    let mut workbook: Xlsx<_> = open_workbook_from_rs::<Xlsx<_>, _>(Cursor::new(bytes))
        .map_err(|error| Error::Decode(error.to_string()))?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or_else(|| Error::Decode("EIA monthly workbook has no first sheet".to_owned()))?
        .map_err(|error| Error::Decode(error.to_string()))?;
    let price_column = match sector {
        "RES" => 7,
        "COM" => 11,
        "IND" => 15,
        _ => return Err(Error::Decode(format!("unsupported EIA sector {sector}"))),
    };
    let mut rows = Vec::new();
    for row in range.rows().skip(3) {
        let year = row
            .first()
            .and_then(|cell| cell.to_string().parse::<i64>().ok());
        let month = row
            .get(1)
            .and_then(|cell| cell.to_string().parse::<i64>().ok());
        let Some(stateid) = row
            .get(2)
            .map(ToString::to_string)
            .filter(|value| value == state)
        else {
            continue;
        };
        let (Some(year), Some(month), Some(price)) = (
            year,
            month,
            row.get(price_column)
                .and_then(|cell| cell.to_string().parse::<f64>().ok()),
        ) else {
            continue;
        };
        rows.push(EiaRow {
            period: format!("{year:04}-{month:02}"),
            stateid,
            sectorid: sector.to_owned(),
            price: Some(price),
        });
    }
    rows.sort_by(|left, right| right.period.cmp(&left.period));
    if rows.is_empty() {
        return Err(Error::Decode(format!(
            "EIA monthly workbook has no {sector} rows for {state}"
        )));
    }
    Ok(rows)
}

/// One state-sector monthly EIA average price record, in cents per kWh.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct EiaRow {
    /// Reporting period as YYYY-MM.
    pub period: String,
    /// State abbreviation.
    pub stateid: String,
    /// Sector identifier.
    pub sectorid: String,
    /// Average price in cents per kWh.
    pub price: Option<f64>,
}

#[derive(Deserialize)]
struct EiaResponse {
    response: EiaData,
}
#[derive(Deserialize)]
struct EiaData {
    data: Vec<EiaRow>,
}

fn retry_delay(attempt: usize) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let cap_ms = 100_u64.saturating_mul(1_u64 << attempt.min(6));
    thread::sleep(Duration::from_millis(u64::from(nanos) % (cap_ms + 1)));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decodes_synthetic_openei_layout() {
        let fixture =
            "zip,eiaid,utility_name,state,service_type,ownership,comm_rate,ind_rate,res_rate\n\
            08840,1,Example Power,NJ,Bundled,Investor Owned,0.120000,0.080000,0.190000\n";
        let row: UtilityRate = csv::Reader::from_reader(fixture.as_bytes())
            .deserialize()
            .next()
            .expect("fixture row")
            .expect("valid row");
        assert_eq!(row.zip_code, "08840");
        assert_eq!(row.res_rate, 0.19);
    }

    #[test]
    fn bulk_download_is_streamed_and_leaves_nothing_in_the_cache_dir() {
        use std::io::Write as _;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}/bulk.csv", listener.local_addr().expect("addr"));
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nbulk")
                .expect("respond");
        });
        let dir = std::env::temp_dir().join(format!("power-bulk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let client = Client::new().with_cache_dir(&dir);
        assert_eq!(client.fetch(&url).expect("downloads"), b"bulk");
        server.join().expect("server exits");
        assert_eq!(client.trail().answered(), Some(Source::Stream));
        assert!(!dir.exists(), "no archive or extract is persisted");
    }
}
