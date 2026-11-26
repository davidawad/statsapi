//! Blocking access to Wikidata event items and Wikipedia Current Events links.

use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::fmt::Write as _;
use std::time::Duration as StdDuration;

/// A normalized event item from Wikidata or a synthetic CSV fixture.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Event {
    /// Event date.
    pub date: NaiveDate,
    /// ISO-3166-1 alpha-3 country code.
    pub country: String,
    /// Optional first-level administrative area.
    pub admin1: String,
    /// Human-readable event title.
    pub title: String,
    /// Human-readable event type.
    pub kind: String,
    /// Optional reported fatalities.
    pub fatalities: Option<u64>,
}

/// A publisher article linked by Wikipedia Current Events.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Article {
    /// Article provenance description.
    pub source: String,
    /// Publisher URL.
    pub url: String,
    /// Why the link is returned.
    pub role: String,
}

/// Errors returned while fetching or parsing Wikimedia data.
#[derive(Debug)]
pub enum Error {
    /// The HTTP request failed.
    Http(reqwest::Error),
    /// The response was not valid JSON.
    Json(serde_json::Error),
    /// CSV parsing failed.
    Csv(csv::Error),
    /// An event date could not be parsed.
    Date(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(error) => write!(formatter, "Wikimedia HTTP request failed: {error}"),
            Self::Json(error) => write!(formatter, "Wikidata response is invalid JSON: {error}"),
            Self::Csv(error) => write!(formatter, "event CSV is invalid: {error}"),
            Self::Date(error) => write!(formatter, "event date is invalid: {error}"),
        }
    }
}

impl std::error::Error for Error {}

/// A blocking Wikimedia client using a caller-provided user agent.
#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
}

impl Client {
    /// Creates a client with a caller-provided user-agent string.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be initialized.
    pub fn new(user_agent: &str) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(user_agent)
            .timeout(StdDuration::from_secs(60))
            .build()
            .map_err(Error::Http)?;
        Ok(Self { http })
    }

    /// Fetches events for a country over the preceding year through Wikidata SPARQL.
    ///
    /// # Errors
    ///
    /// Returns an error when Wikidata cannot be reached or its response is invalid.
    pub fn events(&self, country: &str, today: NaiveDate) -> Result<Vec<Event>, Error> {
        let since = today - Duration::days(365);
        let query = format!(
            "SELECT ?event ?eventLabel ?date ?kindLabel ?deaths WHERE {{ ?country wdt:P298 \"{country}\" . ?event wdt:P17 ?country; wdt:P585 ?date; wdt:P31 ?kind. OPTIONAL {{ ?event wdt:P1120 ?deaths. }} FILTER(?date >= \"{since}T00:00:00Z\"^^xsd:dateTime) SERVICE wikibase:label {{ bd:serviceParam wikibase:language \"en\". }} }} LIMIT 5000"
        );
        let payload: Value = self
            .http
            .get("https://query.wikidata.org/sparql")
            .query(&[("query", query.as_str()), ("format", "json")])
            .header(reqwest::header::ACCEPT, "application/sparql-results+json")
            .send()
            .map_err(Error::Http)?
            .error_for_status()
            .map_err(Error::Http)?
            .json()
            .map_err(Error::Http)?;
        let mut csv =
            String::from("Event Date,Country,Admin1,Event Description,Event Type,Fatalities\n");
        for binding in payload
            .pointer("/results/bindings")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let date = binding
                .pointer("/date/value")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim_start_matches('+')
                .get(..10)
                .unwrap_or("");
            let title = binding
                .pointer("/eventLabel/value")
                .and_then(Value::as_str)
                .unwrap_or("Wikidata event")
                .replace(',', " ");
            let kind = binding
                .pointer("/kindLabel/value")
                .and_then(Value::as_str)
                .unwrap_or("event")
                .replace(',', " ");
            let fatalities = binding
                .pointer("/deaths/value")
                .and_then(Value::as_str)
                .unwrap_or("");
            writeln!(csv, "{date},{country},,{title},{kind},{fatalities}")
                .map_err(|error| Error::Date(error.to_string()))?;
        }
        parse_events(csv.as_bytes())
    }

    /// Fetches publisher links from the Wikipedia Current Events page.
    ///
    /// # Errors
    ///
    /// Returns an error when Wikipedia cannot be reached or its response is invalid.
    pub fn current_events_articles(&self, today: NaiveDate) -> Result<Vec<Article>, Error> {
        let daily_page = format!("Portal:Current_events/{}", today.format("%Y_%B_%-d"));
        let payload: Value = self
            .http
            .get("https://en.wikipedia.org/w/api.php")
            .query(&[
                ("action", "query"),
                ("titles", daily_page.as_str()),
                ("prop", "extlinks"),
                ("ellimit", "50"),
                ("format", "json"),
            ])
            .send()
            .map_err(Error::Http)?
            .error_for_status()
            .map_err(Error::Http)?
            .json()
            .map_err(Error::Http)?;
        Ok(payload
            .pointer("/query/pages")
            .and_then(Value::as_object)
            .and_then(|pages| pages.values().next())
            .and_then(|page| page.get("extlinks"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|link| {
                Some(Article {
                    source: "publisher article linked by Wikipedia Current Events".into(),
                    url: link.get("*")?.as_str()?.to_owned(),
                    role: "article specimen; not counted as a Wikidata event".into(),
                })
            })
            .collect())
    }
}

fn field<'a>(row: &'a csv::StringRecord, headers: &csv::StringRecord, aliases: &[&str]) -> &'a str {
    aliases
        .iter()
        .find_map(|name| {
            headers
                .iter()
                .position(|header| header.eq_ignore_ascii_case(name))
                .and_then(|index| row.get(index))
        })
        .unwrap_or("")
        .trim()
}

/// Parses the adapter's flexible synthetic event CSV format.
///
/// # Errors
///
/// Returns an error when the CSV or one of its dates is invalid.
pub fn parse_events(bytes: &[u8]) -> Result<Vec<Event>, Error> {
    let mut reader = csv::ReaderBuilder::new().flexible(true).from_reader(bytes);
    let headers = reader.headers().map_err(Error::Csv)?.clone();
    let mut events = Vec::new();
    for row in reader.records() {
        let row = row.map_err(Error::Csv)?;
        let raw_date = field(
            &row,
            &headers,
            &["Event Date", "event_date", "date", "SQLDATE"],
        );
        let date = NaiveDate::parse_from_str(raw_date, "%Y-%m-%d")
            .or_else(|_| NaiveDate::parse_from_str(raw_date, "%Y%m%d"))
            .map_err(|error| Error::Date(error.to_string()))?;
        events.push(Event {
            date,
            country: field(
                &row,
                &headers,
                &["Country", "country", "country_iso3", "country_code"],
            )
            .to_ascii_uppercase(),
            admin1: field(&row, &headers, &["Admin1", "admin1", "province", "state"]).to_owned(),
            title: field(
                &row,
                &headers,
                &[
                    "Event Description",
                    "event_description",
                    "title",
                    "event_type",
                ],
            )
            .to_owned(),
            kind: field(
                &row,
                &headers,
                &["Event Type", "event_type", "eventtype", "quad_class"],
            )
            .to_owned(),
            fatalities: field(&row, &headers, &["Fatalities", "fatalities", "deaths"])
                .parse()
                .ok(),
        });
    }
    Ok(events)
}
