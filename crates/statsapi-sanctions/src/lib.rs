//! Blocking, streaming client for the OFAC SDN list and the UN Security
//! Council consolidated list.
//!
//! Both lists are bulk XML files with no query API. The client streams each
//! file over HTTPS, filters it to one country while parsing, and keeps nothing
//! on disk: the largest value held in memory is the set of matching records.

mod countries;
mod xml;

pub use countries::CountryNames;

use reqwest::blocking::Client as HttpClient;
use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::io::{BufRead, BufReader};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use xml::{ends_with, Node};

/// Legacy OFAC SDN export: names, programs, addresses, nationalities.
pub const OFAC_SDN_URL: &str =
    "https://sanctionslistservice.ofac.treas.gov/api/PublicationPreview/exports/SDN.XML";
/// OFAC advanced SDN export: the only one that carries listing dates.
pub const OFAC_ADVANCED_URL: &str =
    "https://sanctionslistservice.ofac.treas.gov/api/PublicationPreview/exports/SDN_ADVANCED.XML";
/// UN Security Council consolidated list.
pub const UN_URL: &str = "https://scsanctions.un.org/resources/xml/en/consolidated.xml";

/// Days an addition stays "recent".
pub const RECENT_DAYS: i64 = 90;

/// Errors returned while fetching or decoding a sanctions list.
#[derive(Debug)]
#[non_exhaustive]
pub enum ClientError {
    /// An input such as a date or country code is malformed.
    InvalidInput(String),
    /// An HTTP request failed after retries.
    Http(reqwest::Error),
    /// The list could not be parsed as XML.
    Xml(quick_xml::Error),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
            Self::Http(error) => write!(f, "sanctions list request failed: {error}"),
            Self::Xml(error) => write!(f, "invalid sanctions list XML: {error}"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Xml(error) => Some(error),
            Self::InvalidInput(_) => None,
        }
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<quick_xml::Error> for ClientError {
    fn from(error: quick_xml::Error) -> Self {
        Self::Xml(error)
    }
}

/// A calendar date, enough to compare listing dates against a cutoff.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Date {
    year: i64,
    month: i64,
    day: i64,
}

impl Date {
    /// Builds a date, rejecting impossible months and days.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::InvalidInput`] outside 1..=12 months or 1..=31 days.
    pub fn new(year: i64, month: i64, day: i64) -> Result<Self, ClientError> {
        if (1..=12).contains(&month) && (1..=31).contains(&day) {
            Ok(Self { year, month, day })
        } else {
            Err(ClientError::InvalidInput(format!(
                "date {year}-{month}-{day}"
            )))
        }
    }

    /// Parses `YYYY-MM-DD`, ignoring any trailing time or zone.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::InvalidInput`] when the text is not a date.
    pub fn parse_iso(text: &str) -> Result<Self, ClientError> {
        let mut parts = text.get(..10).unwrap_or(text).split('-');
        let mut next = || parts.next().and_then(|part| part.parse::<i64>().ok());
        match (next(), next(), next()) {
            (Some(year), Some(month), Some(day)) => Self::new(year, month, day),
            _ => Err(ClientError::InvalidInput(format!("date {text:?}"))),
        }
    }

    /// Parses OFAC's `MM/DD/YYYY`.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::InvalidInput`] when the text is not a date.
    pub fn parse_us(text: &str) -> Result<Self, ClientError> {
        let mut parts = text.trim().split('/');
        let mut next = || parts.next().and_then(|part| part.parse::<i64>().ok());
        match (next(), next(), next()) {
            (Some(month), Some(day), Some(year)) => Self::new(year, month, day),
            _ => Err(ClientError::InvalidInput(format!("date {text:?}"))),
        }
    }

    /// Days since 1970-01-01 in the proleptic Gregorian calendar.
    #[must_use]
    pub fn days(self) -> i64 {
        let year = if self.month <= 2 {
            self.year - 1
        } else {
            self.year
        };
        let era = year.div_euclid(400);
        let year_of_era = year - era * 400;
        let shifted_month = (self.month + 9) % 12;
        let day_of_year = (153 * shifted_month + 2) / 5 + self.day - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }

    /// True when this date is within `days` before `today` (or after it).
    #[must_use]
    pub fn within(self, today: Self, days: i64) -> bool {
        self.days() >= today.days() - days
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// Which list an [`Exposure`] was read from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListKind {
    /// OFAC Specially Designated Nationals list.
    Ofac,
    /// UN Security Council consolidated list.
    Un,
}

/// What one list says about one country.
#[derive(Clone, Debug, PartialEq)]
pub struct Exposure {
    /// The list this was read from.
    pub list: ListKind,
    /// Listed entities with an address (or nationality) in the country.
    pub entities: u32,
    /// Listed individuals with an address or nationality in the country.
    pub individuals: u32,
    /// Matching entities and individuals listed within [`RECENT_DAYS`], when
    /// the dated source could be read.
    pub additions: Option<u32>,
    /// Sanctions programs (OFAC) or committees (UN) touching the country,
    /// most records first, as `(title, records)`.
    pub programs: Vec<(String, u32)>,
    /// The list's own publication date.
    pub published: String,
    /// URLs streamed to produce this value.
    pub source_urls: Vec<String>,
}

impl Exposure {
    fn new(list: ListKind) -> Self {
        Self {
            list,
            entities: 0,
            individuals: 0,
            additions: None,
            programs: Vec::new(),
            published: String::new(),
            source_urls: Vec::new(),
        }
    }

    /// Program titles joined for display, up to `limit` of them.
    #[must_use]
    pub fn program_titles(&self, limit: usize) -> String {
        self.programs
            .iter()
            .take(limit)
            .map(|(title, _)| title.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn ranked(counts: BTreeMap<String, u32>) -> Vec<(String, u32)> {
    let mut ranked: Vec<_> = counts.into_iter().collect();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    ranked
}

/// Result of scanning the OFAC SDN export for one country.
#[derive(Clone, Debug, PartialEq)]
pub struct OfacScan {
    /// Counts and programs; `additions` is still unset.
    pub exposure: Exposure,
    /// `uid` of every matching entity and individual.
    pub listed_uids: Vec<String>,
}

#[derive(Default)]
struct SdnEntry {
    uid: String,
    kind: String,
    programs: Vec<String>,
    matched: bool,
}

/// Scans the legacy OFAC `SDN.XML` stream for entries tied to a country.
///
/// # Errors
///
/// Returns an error when the stream is not well-formed XML.
pub fn scan_ofac_sdn<R: BufRead>(reader: R, names: &CountryNames) -> Result<OfacScan, ClientError> {
    let mut exposure = Exposure::new(ListKind::Ofac);
    let mut listed_uids = Vec::new();
    let mut programs: BTreeMap<String, u32> = BTreeMap::new();
    let mut entry = SdnEntry::default();
    xml::walk(reader, |node, stack| match node {
        Node::Start(_) if ends_with(stack, &["sdnList", "sdnEntry"]) => {
            entry = SdnEntry::default();
        }
        Node::Text(text) => {
            if ends_with(stack, &["sdnEntry", "uid"]) {
                entry.uid = text.to_string();
            } else if ends_with(stack, &["sdnEntry", "sdnType"]) {
                entry.kind = text.to_string();
            } else if ends_with(stack, &["programList", "program"]) {
                entry.programs.push(text.to_string());
            } else if ends_with(stack, &["address", "country"])
                || ends_with(stack, &["nationality", "country"])
            {
                entry.matched |= names.matches(text);
            } else if ends_with(stack, &["publshInformation", "Publish_Date"]) {
                exposure.published =
                    Date::parse_us(text).map_or_else(|_| text.to_string(), |d| d.to_string());
            }
        }
        Node::End if ends_with(stack, &["sdnList", "sdnEntry"]) && entry.matched => {
            match entry.kind.as_str() {
                "Individual" => exposure.individuals += 1,
                "Entity" => exposure.entities += 1,
                _ => {}
            }
            if matches!(entry.kind.as_str(), "Individual" | "Entity") {
                listed_uids.push(std::mem::take(&mut entry.uid));
            }
            for program in std::mem::take(&mut entry.programs) {
                *programs.entry(program).or_default() += 1;
            }
        }
        _ => {}
    })?;
    exposure.programs = ranked(programs);
    Ok(OfacScan {
        exposure,
        listed_uids,
    })
}

/// Reads the `SDN_ADVANCED.XML` stream and returns the profile ids whose
/// listing date falls within [`RECENT_DAYS`] of `today`.
///
/// # Errors
///
/// Returns an error when the stream is not well-formed XML.
pub fn recent_ofac_profiles<R: BufRead>(
    reader: R,
    today: Date,
) -> Result<HashSet<String>, ClientError> {
    let mut recent = HashSet::new();
    let mut profile = String::new();
    let (mut year, mut month, mut day) = (0_i64, 0_i64, 0_i64);
    let mut dated = false;
    xml::walk(reader, |node, stack| match node {
        Node::Start(start) if ends_with(stack, &["SanctionsEntries", "SanctionsEntry"]) => {
            profile = start.attribute("ProfileID").unwrap_or_default();
            dated = false;
        }
        Node::Text(text) if ends_with(stack, &["EntryEvent", "Date", "Year"]) => {
            year = text.parse().unwrap_or(0);
        }
        Node::Text(text) if ends_with(stack, &["EntryEvent", "Date", "Month"]) => {
            month = text.parse().unwrap_or(0);
        }
        Node::Text(text) if ends_with(stack, &["EntryEvent", "Date", "Day"]) => {
            day = text.parse().unwrap_or(0);
        }
        Node::End if ends_with(stack, &["EntryEvent", "Date"]) && !dated => {
            dated = true;
            if Date::new(year, month, day).is_ok_and(|date| date.within(today, RECENT_DAYS)) {
                recent.insert(profile.clone());
            }
        }
        _ => {}
    })?;
    Ok(recent)
}

#[derive(Default)]
struct UnRecord {
    individual: bool,
    list: String,
    listed_on: String,
    matched: bool,
}

/// Scans the UN consolidated list stream for records tied to a country.
///
/// # Errors
///
/// Returns an error when the stream is not well-formed XML.
pub fn scan_un<R: BufRead>(
    reader: R,
    names: &CountryNames,
    today: Date,
) -> Result<Exposure, ClientError> {
    let mut exposure = Exposure::new(ListKind::Un);
    let mut additions = 0;
    let mut programs: BTreeMap<String, u32> = BTreeMap::new();
    let mut record = UnRecord::default();
    xml::walk(reader, |node, stack| match node {
        Node::Start(start) => {
            if ends_with(stack, &["INDIVIDUALS", "INDIVIDUAL"]) {
                record = UnRecord {
                    individual: true,
                    ..UnRecord::default()
                };
            } else if ends_with(stack, &["ENTITIES", "ENTITY"]) {
                record = UnRecord::default();
            } else if ends_with(stack, &["CONSOLIDATED_LIST"]) {
                exposure.published = start
                    .attribute("dateGenerated")
                    .unwrap_or_default()
                    .chars()
                    .take(10)
                    .collect();
            }
        }
        Node::Text(text) => {
            if ends_with(stack, &["INDIVIDUAL", "UN_LIST_TYPE"])
                || ends_with(stack, &["ENTITY", "UN_LIST_TYPE"])
            {
                record.list = text.to_string();
            } else if ends_with(stack, &["INDIVIDUAL", "LISTED_ON"])
                || ends_with(stack, &["ENTITY", "LISTED_ON"])
            {
                record.listed_on = text.to_string();
            } else if ends_with(stack, &["INDIVIDUAL_ADDRESS", "COUNTRY"])
                || ends_with(stack, &["ENTITY_ADDRESS", "COUNTRY"])
                || ends_with(stack, &["NATIONALITY", "VALUE"])
            {
                record.matched |= names.matches(text);
            }
        }
        Node::End
            if (ends_with(stack, &["INDIVIDUALS", "INDIVIDUAL"])
                || ends_with(stack, &["ENTITIES", "ENTITY"]))
                && record.matched =>
        {
            if record.individual {
                exposure.individuals += 1;
            } else {
                exposure.entities += 1;
            }
            if Date::parse_iso(&record.listed_on).is_ok_and(|date| date.within(today, RECENT_DAYS))
            {
                additions += 1;
            }
            *programs
                .entry(std::mem::take(&mut record.list))
                .or_default() += 1;
        }
        Node::End => {}
    })?;
    exposure.additions = Some(additions);
    exposure.programs = ranked(programs);
    exposure.source_urls = vec![UN_URL.to_string()];
    Ok(exposure)
}

/// A blocking client that streams sanctions lists without keeping them.
#[derive(Clone, Debug)]
pub struct SanctionsClient {
    http: HttpClient,
    max_retries: usize,
}

impl SanctionsClient {
    /// Creates a client with a five-minute budget per list and three retries.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be initialized.
    pub fn new() -> Result<Self, ClientError> {
        Ok(Self {
            http: HttpClient::builder()
                .user_agent(concat!("statsapi-sanctions/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(300))
                .build()?,
            max_retries: 3,
        })
    }

    /// Sets the number of retries after the initial attempt.
    #[must_use]
    pub fn with_max_retries(mut self, retries: usize) -> Self {
        self.max_retries = retries;
        self
    }

    fn open(&self, url: &str) -> Result<BufReader<reqwest::blocking::Response>, ClientError> {
        for attempt in 0..=self.max_retries {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    return Ok(BufReader::with_capacity(1 << 16, response));
                }
                Ok(response) if !retryable_status(response.status().as_u16()) => {
                    return Err(response.error_for_status().map_or_else(Into::into, |_| {
                        ClientError::InvalidInput("unexpected success".into())
                    }));
                }
                Ok(_) | Err(_) if attempt < self.max_retries => jittered_delay(attempt),
                Ok(response) => {
                    return Err(response.error_for_status().map_or_else(Into::into, |_| {
                        ClientError::InvalidInput("unexpected success".into())
                    }));
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(ClientError::InvalidInput("retry loop ended".into()))
    }

    /// Streams the OFAC SDN list (and, for listing dates, the advanced
    /// export) and reports one country's exposure as of `today`.
    ///
    /// `additions` stays `None` when the dated export cannot be read.
    ///
    /// # Errors
    ///
    /// Returns an error when the SDN stream cannot be fetched or parsed.
    pub fn ofac_exposure(
        &self,
        names: &CountryNames,
        today: Date,
    ) -> Result<Exposure, ClientError> {
        let scan = scan_ofac_sdn(self.open(OFAC_SDN_URL)?, names)?;
        let mut exposure = scan.exposure;
        exposure.source_urls = vec![OFAC_SDN_URL.to_string()];
        if let Ok(recent) = self
            .open(OFAC_ADVANCED_URL)
            .and_then(|reader| recent_ofac_profiles(reader, today))
        {
            let count = scan
                .listed_uids
                .iter()
                .filter(|uid| recent.contains(*uid))
                .count();
            exposure.additions = u32::try_from(count).ok();
            exposure.source_urls.push(OFAC_ADVANCED_URL.to_string());
        }
        Ok(exposure)
    }

    /// Streams the UN consolidated list and reports one country's exposure.
    ///
    /// # Errors
    ///
    /// Returns an error when the list cannot be fetched or parsed.
    pub fn un_exposure(&self, names: &CountryNames, today: Date) -> Result<Exposure, ClientError> {
        scan_un(self.open(UN_URL)?, names, today)
    }
}

fn retryable_status(status: u16) -> bool {
    status == 429 || (500..=599).contains(&status)
}

fn jittered_delay(attempt: usize) {
    let base_ms = 200_u64.saturating_mul(1_u64 << attempt.min(5));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let delay = base_ms / 2 + u64::from(nanos) % base_ms;
    std::thread::sleep(Duration::from_millis(delay));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> Date {
        Date::parse_iso("2026-11-23").expect("date")
    }

    #[test]
    fn days_matches_known_epochs() {
        assert_eq!(Date::new(1970, 1, 1).expect("date").days(), 0);
        assert_eq!(Date::new(2000, 3, 1).expect("date").days(), 11_017);
        assert_eq!(
            today().days() - Date::parse_iso("2026-08-25").expect("date").days(),
            90
        );
    }

    #[test]
    fn within_uses_a_ninety_day_window() {
        assert!(Date::parse_iso("2026-08-25")
            .expect("date")
            .within(today(), 90));
        assert!(!Date::parse_iso("2026-08-24")
            .expect("date")
            .within(today(), 90));
    }

    #[test]
    fn parses_us_and_iso_dates() {
        assert_eq!(
            Date::parse_us("11/20/2026").expect("date").to_string(),
            "2026-11-20"
        );
        assert_eq!(
            Date::parse_iso("2026-11-22T00:00:00.942Z")
                .expect("date")
                .to_string(),
            "2026-11-22"
        );
        assert!(Date::parse_iso("garbage").is_err());
    }

    const SDN: &str = include_str!("../tests/fixtures/sdn.xml");
    const ADVANCED: &str = include_str!("../tests/fixtures/sdn_advanced.xml");
    const UN: &str = include_str!("../tests/fixtures/un.xml");

    #[test]
    fn ofac_scan_counts_address_and_nationality_matches() {
        let names = CountryNames::for_alpha3("IRN").expect("IRN");
        let scan = scan_ofac_sdn(SDN.as_bytes(), &names).expect("scan");
        assert_eq!(scan.exposure.entities, 1);
        assert_eq!(scan.exposure.individuals, 2);
        assert_eq!(scan.listed_uids, ["1001", "1002", "1003"]);
        assert_eq!(
            scan.exposure.programs,
            [("IRAN".to_string(), 3), ("SDGT".to_string(), 1)]
        );
        assert_eq!(scan.exposure.published, "2026-11-20");
    }

    #[test]
    fn ofac_scan_ignores_other_countries() {
        let names = CountryNames::for_alpha3("USA").expect("USA");
        let scan = scan_ofac_sdn(SDN.as_bytes(), &names).expect("scan");
        assert_eq!(scan.exposure.entities + scan.exposure.individuals, 0);
        assert_eq!(scan.exposure.programs.len(), 0);
    }

    #[test]
    fn advanced_export_yields_recent_profiles_only() {
        let recent = recent_ofac_profiles(ADVANCED.as_bytes(), today()).expect("recent");
        let mut ids: Vec<_> = recent.into_iter().collect();
        ids.sort();
        assert_eq!(ids, ["1002", "1004"]);
    }

    #[test]
    fn un_scan_counts_nationality_and_address_with_additions() {
        let names = CountryNames::for_alpha3("COD").expect("COD");
        let exposure = scan_un(UN.as_bytes(), &names, today()).expect("scan");
        assert_eq!(exposure.individuals, 1);
        assert_eq!(exposure.entities, 1);
        assert_eq!(exposure.additions, Some(1));
        assert_eq!(exposure.programs, [("DRC".to_string(), 2)]);
        assert_eq!(exposure.published, "2026-11-22");
    }

    #[test]
    #[ignore = "streams the live OFAC and UN lists"]
    fn live_lists_report_iran_exposure() {
        let client = SanctionsClient::new().expect("client");
        let names = CountryNames::for_alpha3("IRN").expect("IRN");
        let now = Date::parse_iso(
            &std::env::var("SANCTIONS_TODAY").unwrap_or_else(|_| "2026-11-23".into()),
        )
        .expect("today");
        let ofac = client.ofac_exposure(&names, now).expect("ofac");
        assert!(ofac.entities > 0 && ofac.individuals > 0);
        assert!(ofac.additions.is_some());
        let un = client.un_exposure(&names, now).expect("un");
        assert!(un.entities + un.individuals > 0);
    }
}
