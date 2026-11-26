//! Streaming client for the SBA open data loan files (<https://data.sba.gov/>).
//!
//! The portal publishes the 7(a), 504 and PPP loan-level FOIA files as large CSVs. This client
//! never stores them: it discovers the current file URLs from the dataset pages, streams each
//! response through the CSV decoder row by row, and hands typed rows to a caller closure.

use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_RETRIES: u32 = 3;

/// The 7(a) and 504 dataset page that lists the loan CSVs.
pub const SEVEN_A_504_PAGE: &str = "https://data.sba.gov/dataset/7a-504-foia";
/// The PPP dataset page that lists the loan CSVs.
pub const PPP_PAGE: &str = "https://data.sba.gov/dataset/ppp-foia";

/// Errors returned while discovering or streaming SBA files.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The HTTP request failed.
    #[error("SBA request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// SBA returned a non-success status.
    #[error("SBA returned HTTP {status} for {url}")]
    Status {
        /// Requested URL.
        url: String,
        /// HTTP status code.
        status: u16,
    },
    /// A CSV row could not be decoded.
    #[error("SBA CSV is invalid: {0}")]
    Csv(#[from] csv::Error),
    /// A required column is absent from the CSV header.
    #[error("SBA CSV is missing column {0}")]
    MissingColumn(&'static str),
    /// The dataset page did not list a file this client needs.
    #[error("SBA dataset page lists no {0} file")]
    NoResource(&'static str),
    /// The stream ended or failed while being read.
    #[error("SBA stream read failed: {0}")]
    Read(#[from] std::io::Error),
}

/// Which loan program a row belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Program {
    /// SBA 7(a) general small business loans.
    SevenA,
    /// SBA 504 CDC/third-party real estate and equipment loans.
    FiveOhFour,
}

/// One 7(a) or 504 loan approval.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Loan {
    /// Loan program.
    pub program: Program,
    /// Borrower ZIP, first five digits.
    pub zip: String,
    /// Project county name as published (upper or mixed case).
    pub county: String,
    /// Project state abbreviation.
    pub state: String,
    /// Gross approval in dollars.
    pub gross_approval: f64,
    /// Approval date, `YYYY-MM-DD`.
    pub approval_date: String,
    /// NAICS code of the borrower.
    pub naics_code: String,
    /// NAICS title of the borrower.
    pub naics_description: String,
}

/// One PPP loan.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PppLoan {
    /// Project ZIP, first five digits (borrower ZIP when the project ZIP is blank).
    pub zip: String,
    /// Project county name as published.
    pub county: String,
    /// Project state abbreviation.
    pub state: String,
    /// Current approval amount in dollars.
    pub amount: f64,
}

/// The current file URLs found on the dataset pages.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Resources {
    /// 7(a) loans, newest fiscal-year file (FY2020 to present).
    pub seven_a: String,
    /// 504 loans, FY2010 to present.
    pub five_oh_four: String,
    /// Every PPP shard.
    pub ppp: Vec<String>,
}

/// Extracts every `href` that ends in `.csv` from a dataset page.
#[must_use]
pub fn csv_links(html: &str) -> Vec<String> {
    let mut links: Vec<String> = Vec::new();
    for part in html.split("href=\"").skip(1) {
        let Some(end) = part.find('"') else { continue };
        let url = &part[..end];
        let is_csv = std::path::Path::new(url)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"));
        if is_csv && !links.iter().any(|seen| seen == url) {
            links.push(url.to_owned());
        }
    }
    links
}

/// Picks the files this client reads out of the two dataset pages' CSV links.
///
/// # Errors
/// Returns [`Error::NoResource`] when a needed file is not listed.
pub fn pick_resources(
    seven_a_504_page: &[String],
    ppp_page: &[String],
) -> Result<Resources, Error> {
    let find = |links: &[String], needle: &str, label: &'static str| {
        links
            .iter()
            .find(|url| url.contains(needle))
            .cloned()
            .ok_or(Error::NoResource(label))
    };
    let ppp: Vec<String> = ppp_page
        .iter()
        .filter(|url| url.contains("/public_"))
        .cloned()
        .collect();
    if ppp.is_empty() {
        return Err(Error::NoResource("PPP"));
    }
    Ok(Resources {
        seven_a: find(
            seven_a_504_page,
            "FOIA_7a_FY2020_Present",
            "7(a) FY2020-present",
        )?,
        five_oh_four: find(
            seven_a_504_page,
            "FOIA_504_FY2010_Present",
            "504 FY2010-present",
        )?,
        ppp,
    })
}

/// The as-of stamp (`YYMMDD`) at the end of a file name, e.g. `260930` for
/// `FOIA_504_FY2010_Present_asof_260930.csv` or `public_150k_plus_240930.csv`.
#[must_use]
pub fn as_of_stamp(url: &str) -> Option<&str> {
    let stem = url.rsplit('/').next()?.strip_suffix(".csv")?;
    let (_, stamp) = stem.rsplit_once('_')?;
    (stamp.len() == 6 && stamp.bytes().all(|byte| byte.is_ascii_digit())).then_some(stamp)
}

/// Expands a `YYMMDD` stamp to `20YY-MM-DD`.
#[must_use]
pub fn stamp_date(stamp: &str) -> String {
    format!("20{}-{}-{}", &stamp[..2], &stamp[2..4], &stamp[4..6])
}

/// First five characters of a ZIP, empty when it is not five digits.
#[must_use]
pub fn zip5(raw: &str) -> String {
    let head: String = raw.trim().chars().take(5).collect();
    if head.len() == 5 && head.bytes().all(|byte| byte.is_ascii_digit()) {
        head
    } else {
        String::new()
    }
}

struct Columns(Vec<String>);
impl Columns {
    fn index(&self, name: &'static str) -> Result<usize, Error> {
        self.0
            .iter()
            .position(|header| header == name)
            .ok_or(Error::MissingColumn(name))
    }
    fn find(&self, name: &str) -> Option<usize> {
        self.0.iter().position(|header| header == name)
    }
}

fn reader<R: Read>(source: R) -> csv::Reader<R> {
    csv::ReaderBuilder::new().flexible(true).from_reader(source)
}

fn headers<R: Read>(csv: &mut csv::Reader<R>) -> Result<Columns, Error> {
    Ok(Columns(
        csv.byte_headers()?
            .iter()
            .map(|name| String::from_utf8_lossy(name).trim().to_owned())
            .collect(),
    ))
}

/// One trimmed field. The PPP files are not always UTF-8, so bytes decode lossily.
fn field(record: &csv::ByteRecord, index: usize) -> std::borrow::Cow<'_, str> {
    record.get(index).map_or(
        std::borrow::Cow::Borrowed(""),
        |bytes| match String::from_utf8_lossy(bytes) {
            std::borrow::Cow::Borrowed(text) => std::borrow::Cow::Borrowed(text.trim()),
            std::borrow::Cow::Owned(text) => std::borrow::Cow::Owned(text.trim().to_owned()),
        },
    )
}

/// Decodes a 7(a) or 504 FOIA CSV, calling `each` per row; rows with a blank approval date are
/// skipped.
///
/// # Errors
/// Returns an error when a required column is absent or the stream fails.
pub fn decode_loans<R: Read>(source: R, mut each: impl FnMut(Loan)) -> Result<(), Error> {
    let mut csv = reader(source);
    let columns = headers(&mut csv)?;
    let program = columns.index("Program")?;
    let zip = columns.index("BorrZip")?;
    let county = columns.index("ProjectCounty")?;
    let state = columns.index("ProjectState")?;
    let gross = columns.index("GrossApproval")?;
    let date = columns.index("ApprovalDate")?;
    let naics = columns.index("NaicsCode")?;
    let title = columns.index("NaicsDescription")?;
    let mut record = csv::ByteRecord::new();
    while csv.read_byte_record(&mut record)? {
        let approval_date = field(&record, date).into_owned();
        if approval_date.is_empty() {
            continue;
        }
        let program = if field(&record, program).eq_ignore_ascii_case("504") {
            Program::FiveOhFour
        } else {
            Program::SevenA
        };
        each(Loan {
            program,
            zip: zip5(&field(&record, zip)),
            county: field(&record, county).into_owned(),
            state: field(&record, state).to_ascii_uppercase(),
            gross_approval: field(&record, gross).parse().unwrap_or(0.0),
            approval_date,
            naics_code: field(&record, naics).into_owned(),
            naics_description: field(&record, title).into_owned(),
        });
    }
    Ok(())
}

/// Decodes a PPP CSV, calling `each` per row.
///
/// # Errors
/// Returns an error when a required column is absent or the stream fails.
pub fn decode_ppp<R: Read>(source: R, mut each: impl FnMut(PppLoan)) -> Result<(), Error> {
    let mut csv = reader(source);
    let columns = headers(&mut csv)?;
    let project_zip = columns.index("ProjectZip")?;
    let borrower_zip = columns.find("BorrowerZip");
    let county = columns.index("ProjectCountyName")?;
    let state = columns.index("ProjectState")?;
    let amount = columns.index("CurrentApprovalAmount")?;
    let mut record = csv::ByteRecord::new();
    while csv.read_byte_record(&mut record)? {
        let mut zip = zip5(&field(&record, project_zip));
        if zip.is_empty() {
            zip = borrower_zip.map_or_else(String::new, |at| zip5(&field(&record, at)));
        }
        each(PppLoan {
            zip,
            county: field(&record, county).into_owned(),
            state: field(&record, state).to_ascii_uppercase(),
            amount: field(&record, amount).parse().unwrap_or(0.0),
        });
    }
    Ok(())
}

/// Blocking SBA client. It keeps nothing on disk.
#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
}

impl Client {
    /// Builds a client whose requests (including whole streamed bodies) end after `timeout`.
    ///
    /// # Errors
    /// Returns an error when the HTTP client cannot be built.
    pub fn new(timeout: Duration) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .user_agent(concat!("statsapi-sba/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { http })
    }

    fn open(&self, url: &str) -> Result<reqwest::blocking::Response, Error> {
        for attempt in 0..=MAX_RETRIES {
            let retry = match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    let status = response.status();
                    if !(status.as_u16() == 429 || status.is_server_error()) {
                        return Err(Error::Status {
                            url: url.to_owned(),
                            status: status.as_u16(),
                        });
                    }
                    Error::Status {
                        url: url.to_owned(),
                        status: status.as_u16(),
                    }
                }
                Err(error) => Error::Http(error),
            };
            if attempt == MAX_RETRIES {
                return Err(retry);
            }
            let jitter = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |now| u64::from(now.subsec_millis()) % 250);
            std::thread::sleep(Duration::from_millis(
                250_u64.saturating_mul(1 << attempt) + jitter,
            ));
        }
        Err(Error::NoResource("response"))
    }

    /// Fetches a small text document (a dataset page or code list) into memory.
    ///
    /// # Errors
    /// Returns an error for HTTP failures.
    pub fn text(&self, url: &str) -> Result<String, Error> {
        Ok(self.open(url)?.text()?)
    }

    /// Reads the two dataset pages and returns the current file URLs.
    ///
    /// # Errors
    /// Returns an error when a page cannot be read or lacks a needed file.
    pub fn discover(&self) -> Result<Resources, Error> {
        let loans = csv_links(&self.text(SEVEN_A_504_PAGE)?);
        let ppp = csv_links(&self.text(PPP_PAGE)?);
        pick_resources(&loans, &ppp)
    }

    /// Streams a 7(a) or 504 CSV, calling `each` per loan without storing the file.
    ///
    /// # Errors
    /// Returns an error for HTTP, header or stream failures.
    pub fn stream_loans(&self, url: &str, each: impl FnMut(Loan)) -> Result<(), Error> {
        decode_loans(self.open(url)?, each)
    }

    /// Streams a PPP CSV, calling `each` per loan without storing the file.
    ///
    /// # Errors
    /// Returns an error for HTTP, header or stream failures.
    pub fn stream_ppp(&self, url: &str, each: impl FnMut(PppLoan)) -> Result<(), Error> {
        decode_ppp(self.open(url)?, each)
    }
}
