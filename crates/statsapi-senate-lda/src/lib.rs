//! Blocking client for the US Senate Lobbying Disclosure Act (LDA) REST API.
//!
//! The anonymous tier is keyless and rate limited, so the client counts with
//! one-row pages, pages through filings only on request, and honours
//! `Retry-After` on HTTP 429.

use reqwest::blocking::Client as HttpClient;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::thread;
use std::time::Duration;

const BASE: &str = "https://lda.senate.gov/api/v1/filings/";
/// Largest page the API serves to anonymous callers.
const PAGE_SIZE: u32 = 25;
/// Retries for HTTP 429, HTTP 5xx and transport errors.
const ATTEMPTS: u32 = 4;
/// Longest pause honoured from a `Retry-After` header, in seconds.
const MAX_RETRY_WAIT_S: u64 = 30;

/// A calendar quarter, which the LDA API calls a `filing_period`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Quarter {
    /// January to March.
    First,
    /// April to June.
    Second,
    /// July to September.
    Third,
    /// October to December.
    Fourth,
}

impl Quarter {
    fn number(self) -> u32 {
        match self {
            Self::First => 1,
            Self::Second => 2,
            Self::Third => 3,
            Self::Fourth => 4,
        }
    }

    fn period(self) -> &'static str {
        match self {
            Self::First => "first_quarter",
            Self::Second => "second_quarter",
            Self::Third => "third_quarter",
            Self::Fourth => "fourth_quarter",
        }
    }

    /// The original quarterly report type code (`Q1` to `Q4`); amendments end in `A`.
    #[must_use]
    pub fn report_type(self) -> String {
        format!("Q{}", self.number())
    }

    fn from_month(month: u32) -> Self {
        match month {
            1..=3 => Self::First,
            4..=6 => Self::Second,
            7..=9 => Self::Third,
            _ => Self::Fourth,
        }
    }
}

impl fmt::Display for Quarter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Q{}", self.number())
    }
}

/// One reporting quarter of one year.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Period {
    /// Calendar year of the filing period.
    pub year: i32,
    /// Quarter within the year.
    pub quarter: Quarter,
}

impl fmt::Display for Period {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.year, self.quarter)
    }
}

/// The four most recent completed quarters before `today` (`YYYY-MM-DD`), oldest first.
///
/// The quarter containing `today` is excluded: its reports are still being filed.
///
/// # Errors
///
/// Returns [`ClientError::InvalidQuery`] when `today` is not a `YYYY-MM-DD` date.
pub fn last_four_quarters(today: &str) -> Result<Vec<Period>, ClientError> {
    let invalid = || ClientError::InvalidQuery(format!("today must be YYYY-MM-DD, got {today:?}"));
    let mut parts = today.split('-');
    let year: i32 = parts
        .next()
        .and_then(|part| part.parse().ok())
        .ok_or_else(invalid)?;
    let month: u32 = parts
        .next()
        .and_then(|part| part.parse().ok())
        .filter(|month| (1..=12).contains(month))
        .ok_or_else(invalid)?;
    let (mut year, mut quarter) = (year, Quarter::from_month(month));
    let mut periods = Vec::with_capacity(4);
    for _ in 0..4 {
        (year, quarter) = match quarter {
            Quarter::First => (year - 1, Quarter::Fourth),
            Quarter::Second => (year, Quarter::First),
            Quarter::Third => (year, Quarter::Second),
            Quarter::Fourth => (year, Quarter::Third),
        };
        periods.push(Period { year, quarter });
    }
    periods.reverse();
    Ok(periods)
}

/// Which filings to read: one period, optionally narrowed by state address.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilingQuery {
    period: Period,
    registrant_state: Option<String>,
    client_state: Option<String>,
}

impl FilingQuery {
    /// Creates a query for the original quarterly reports of `period`.
    #[must_use]
    pub fn new(period: Period) -> Self {
        Self {
            period,
            registrant_state: None,
            client_state: None,
        }
    }

    /// Keeps filings whose registrant (lobbying firm or in-house filer) is in `state`.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::InvalidQuery`] unless `state` is two uppercase letters.
    pub fn registrant_state(mut self, state: &str) -> Result<Self, ClientError> {
        self.registrant_state = Some(state_code(state)?);
        Ok(self)
    }

    /// Keeps filings whose client is in `state`.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::InvalidQuery`] unless `state` is two uppercase letters.
    pub fn client_state(mut self, state: &str) -> Result<Self, ClientError> {
        self.client_state = Some(state_code(state)?);
        Ok(self)
    }

    /// Returns the period this query reads.
    #[must_use]
    pub fn period(&self) -> Period {
        self.period
    }

    /// Builds the request URL for a page size.
    ///
    /// Every value is a digit, an ASCII letter or an underscore, so none needs escaping.
    #[must_use]
    pub fn url(&self, page_size: u32) -> String {
        let mut url = format!(
            "{BASE}?filing_year={}&filing_period={}&filing_type={}",
            self.period.year,
            self.period.quarter.period(),
            self.period.quarter.report_type()
        );
        if let Some(state) = &self.registrant_state {
            url.push_str("&registrant_state=");
            url.push_str(state);
        }
        if let Some(state) = &self.client_state {
            url.push_str("&client_state=");
            url.push_str(state);
        }
        url.push_str("&page_size=");
        url.push_str(&page_size.to_string());
        url
    }
}

fn state_code(state: &str) -> Result<String, ClientError> {
    if state.len() == 2 && state.bytes().all(|byte| byte.is_ascii_uppercase()) {
        Ok(state.to_string())
    } else {
        Err(ClientError::InvalidQuery(format!(
            "state must be two uppercase letters, got {state:?}"
        )))
    }
}

/// One lobbying filing reduced to the fields this client reports.
#[derive(Clone, Debug, PartialEq)]
pub struct Filing {
    /// Report type code such as `Q1` (amendments end in `A`).
    pub filing_type: String,
    /// Income a lobbying firm reported from the client, in USD.
    pub income: Option<f64>,
    /// Expenses an in-house filer reported, in USD.
    pub expenses: Option<f64>,
    /// `(general_issue_code, display name)` for each lobbying activity.
    pub issues: Vec<(String, String)>,
}

impl Filing {
    /// Spending this filing reports: firm income plus in-house expenses.
    ///
    /// A filing carries one or the other, so the sum never double counts.
    #[must_use]
    pub fn spending(&self) -> f64 {
        self.income.unwrap_or(0.0) + self.expenses.unwrap_or(0.0)
    }
}

/// One page of the paginated filings response.
#[derive(Clone, Debug, PartialEq)]
pub struct FilingsPage {
    /// Total filings matching the query across all pages.
    pub count: u64,
    /// URL of the next page, when there is one.
    pub next: Option<String>,
    /// Filings on this page.
    pub filings: Vec<Filing>,
}

fn money(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::String(text) => text.trim().parse().ok(),
        Value::Number(number) => number.as_f64(),
        _ => None,
    }
}

/// Decodes one page of the `/filings/` response.
///
/// # Errors
///
/// Returns [`ClientError::Shape`] when `count` or `results` is missing.
pub fn parse_page(payload: &Value) -> Result<FilingsPage, ClientError> {
    let count = payload
        .get("count")
        .and_then(Value::as_u64)
        .ok_or_else(|| ClientError::Shape("missing count".into()))?;
    let results = payload
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| ClientError::Shape("missing results".into()))?;
    let next = payload
        .get("next")
        .and_then(Value::as_str)
        .map(str::to_string);
    let filings = results
        .iter()
        .map(|row| Filing {
            filing_type: row
                .get("filing_type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            income: money(row.get("income")),
            expenses: money(row.get("expenses")),
            issues: row
                .get("lobbying_activities")
                .and_then(Value::as_array)
                .map_or_else(Vec::new, |activities| {
                    activities
                        .iter()
                        .filter_map(|activity| {
                            let code = activity.get("general_issue_code")?.as_str()?;
                            let display = activity
                                .get("general_issue_code_display")
                                .and_then(Value::as_str)
                                .unwrap_or(code);
                            Some((code.to_string(), display.to_string()))
                        })
                        .collect()
                }),
        })
        .collect();
    Ok(FilingsPage {
        count,
        next,
        filings,
    })
}

/// Totals over every filing read for a set of periods.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    /// Original quarterly reports counted.
    pub filings: u64,
    /// Sum of reported firm income and in-house expenses, in USD.
    pub spending: f64,
    /// Filings naming each `(code, display)` issue, counted once per filing.
    pub issues: BTreeMap<(String, String), u64>,
}

impl Summary {
    /// Adds the filings of one page, skipping amendments and repeated issue codes.
    pub fn add(&mut self, page: &FilingsPage, report_type: &str) {
        for filing in page.filings.iter().filter(|f| f.filing_type == report_type) {
            self.filings += 1;
            self.spending += filing.spending();
            let mut seen: Vec<&(String, String)> = Vec::new();
            for issue in &filing.issues {
                if !seen.contains(&issue) {
                    seen.push(issue);
                    *self.issues.entry(issue.clone()).or_default() += 1;
                }
            }
        }
    }

    /// The most frequent issue as `(code, display, filings)`; ties go to the lower code.
    #[must_use]
    pub fn top_issue(&self) -> Option<(&str, &str, u64)> {
        self.issues
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
            .map(|((code, display), count)| (code.as_str(), display.as_str(), *count))
    }
}

/// Errors returned while building or fetching an LDA response.
#[derive(Debug)]
#[non_exhaustive]
pub enum ClientError {
    /// A query parameter is malformed.
    InvalidQuery(String),
    /// An HTTP request failed after retries.
    Http(reqwest::Error),
    /// The API answered with a status other than success.
    Status(u16),
    /// The response body was not valid JSON.
    Json(serde_json::Error),
    /// The JSON lacked an expected field.
    Shape(String),
    /// The query matches more filings than the caller's page budget reads.
    Budget {
        /// Filings the query matches.
        filings: u64,
        /// Largest number of pages the caller allowed.
        max_pages: u32,
    },
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQuery(message) => write!(f, "invalid query: {message}"),
            Self::Http(error) => write!(f, "LDA request failed: {error}"),
            Self::Status(code) => write!(f, "LDA API answered HTTP {code}"),
            Self::Json(error) => write!(f, "invalid LDA JSON: {error}"),
            Self::Shape(message) => write!(f, "unexpected LDA response: {message}"),
            Self::Budget { filings, max_pages } => write!(
                f,
                "{filings} filings exceed the {max_pages}-page budget of the anonymous tier"
            ),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::InvalidQuery(_) | Self::Status(_) | Self::Shape(_) | Self::Budget { .. } => None,
        }
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Blocking LDA API client; every call queries the live API and persists nothing.
#[derive(Clone, Debug)]
pub struct LdaClient {
    http: HttpClient,
}

impl LdaClient {
    /// Creates a client with a 30 second timeout.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::Http`] when the TLS backend cannot start.
    pub fn new() -> Result<Self, ClientError> {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("statsapi-senate-lda/0.1 (+https://github.com/davidawad/statsapi)")
            .build()?;
        Ok(Self { http })
    }

    /// Fetches and decodes one URL, retrying 429, 5xx and transport errors.
    ///
    /// # Errors
    ///
    /// Returns the last transport, status or JSON error.
    pub fn get_page(&self, url: &str) -> Result<FilingsPage, ClientError> {
        let mut last = ClientError::Status(0);
        for attempt in 0..ATTEMPTS {
            if attempt > 0 {
                thread::sleep(Duration::from_secs(u64::from(attempt) * 2));
            }
            match self.http.get(url).send() {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return parse_page(&serde_json::from_str(&response.text()?)?);
                    }
                    last = ClientError::Status(status.as_u16());
                    if status.as_u16() == 429 {
                        let wait = response
                            .headers()
                            .get("retry-after")
                            .and_then(|value| value.to_str().ok())
                            .and_then(|value| value.trim().parse::<u64>().ok())
                            .unwrap_or(5)
                            .min(MAX_RETRY_WAIT_S);
                        thread::sleep(Duration::from_secs(wait));
                    } else if !status.is_server_error() {
                        return Err(last);
                    }
                }
                Err(error) => last = ClientError::Http(error),
            }
        }
        Err(last)
    }

    /// Counts the filings a query matches with a one-row request.
    ///
    /// # Errors
    ///
    /// Returns any [`ClientError`] from the request.
    pub fn count(&self, query: &FilingQuery) -> Result<u64, ClientError> {
        Ok(self.get_page(&query.url(1))?.count)
    }

    /// Reads every filing of a query into `summary`, within `max_pages` requests.
    ///
    /// The first page reports the total, so a query over budget fails before
    /// reading the rest and `summary` is left untouched.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::Budget`] when the query needs more than `max_pages`
    /// pages, or any request error.
    pub fn summarize(
        &self,
        query: &FilingQuery,
        max_pages: u32,
        summary: &mut Summary,
    ) -> Result<(), ClientError> {
        let first = self.get_page(&query.url(PAGE_SIZE))?;
        let needed = first.count.div_ceil(u64::from(PAGE_SIZE));
        if needed > u64::from(max_pages) {
            return Err(ClientError::Budget {
                filings: first.count,
                max_pages,
            });
        }
        let report_type = query.period.quarter.report_type();
        let mut staged = Summary::default();
        staged.add(&first, &report_type);
        let mut next = first.next;
        while let Some(url) = next {
            let page = self.get_page(&url)?;
            staged.add(&page, &report_type);
            next = page.next;
        }
        summary.filings += staged.filings;
        summary.spending += staged.spending;
        for (issue, count) in staged.issues {
            *summary.issues.entry(issue).or_default() += count;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/filings-page.json"))
            .expect("fixture JSON")
    }

    #[test]
    fn quarters_before_a_mid_year_date_wrap_the_year() {
        let periods = last_four_quarters("2026-02-10").expect("valid date");
        let labels: Vec<String> = periods.iter().map(ToString::to_string).collect();
        assert_eq!(labels, ["2025 Q1", "2025 Q2", "2025 Q3", "2025 Q4"]);
    }

    #[test]
    fn current_quarter_is_excluded() {
        let periods = last_four_quarters("2026-11-23").expect("valid date");
        let labels: Vec<String> = periods.iter().map(ToString::to_string).collect();
        assert_eq!(labels, ["2025 Q4", "2026 Q1", "2026 Q2", "2026 Q3"]);
    }

    #[test]
    fn malformed_dates_are_rejected() {
        for bad in ["unknown", "2026", "2026-13-01", ""] {
            assert!(last_four_quarters(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn url_carries_period_type_and_state_filters() {
        let period = Period {
            year: 2025,
            quarter: Quarter::Second,
        };
        let url = FilingQuery::new(period)
            .client_state("NJ")
            .expect("state")
            .url(25);
        assert_eq!(
            url,
            "https://lda.senate.gov/api/v1/filings/?filing_year=2025&filing_period=second_quarter&filing_type=Q2&client_state=NJ&page_size=25"
        );
    }

    #[test]
    fn state_must_be_two_uppercase_letters() {
        let period = Period {
            year: 2025,
            quarter: Quarter::First,
        };
        assert!(FilingQuery::new(period).client_state("nj").is_err());
        assert!(FilingQuery::new(period).registrant_state("NJX").is_err());
    }

    #[test]
    fn page_decodes_money_issues_and_next_link() {
        let page = parse_page(&fixture()).expect("page");
        assert_eq!(page.count, 3);
        assert!(page.next.is_some());
        assert_eq!(page.filings.len(), 3);
        assert_eq!(page.filings[0].income, Some(40000.0));
        assert_eq!(page.filings[1].expenses, Some(125_000.5));
        assert_eq!(page.filings[1].income, None);
    }

    #[test]
    fn summary_skips_amendments_and_counts_each_issue_once_per_filing() {
        let page = parse_page(&fixture()).expect("page");
        let mut summary = Summary::default();
        summary.add(&page, "Q2");
        assert_eq!(summary.filings, 2);
        assert!((summary.spending - 165_000.5).abs() < 1e-6);
        let top = summary.top_issue().expect("issues");
        assert_eq!(top, ("TAX", "Taxes", 2));
    }

    #[test]
    fn page_without_results_is_a_shape_error() {
        assert!(matches!(
            parse_page(&serde_json::json!({"count": 1})),
            Err(ClientError::Shape(_))
        ));
    }
}
