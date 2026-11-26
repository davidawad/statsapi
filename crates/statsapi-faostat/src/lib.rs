//! Blocking client for FAOSTAT's bulk "normalized" datasets.
//!
//! FAOSTAT's JSON API (`faostatservices.fao.org`) now requires a login and the
//! older `fenixservices` endpoint is unavailable, so this client streams the
//! public bulk zip into memory, filters one country and the requested
//! item/element pairs while reading it, and persists nothing.

mod iso;

use reqwest::blocking::Client as HttpClient;
use std::fmt;
use std::io::{Cursor, Read};
use std::time::Duration;

const BULK_BASE: &str = "https://bulks-faostat.fao.org/production";
/// Largest bulk zip the client will hold in memory.
const MAX_ZIP_BYTES: u64 = 128 * 1024 * 1024;

/// A FAOSTAT domain served as a bulk "All Data (Normalized)" zip.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Dataset {
    /// `QCL`: crops and livestock products (production, area, yield).
    CropsLivestock,
    /// `FS`: suite of food security indicators.
    FoodSecurity,
    /// `CP`: consumer price indices, including food indices and inflation.
    ConsumerPrices,
}

impl Dataset {
    /// Parses a FAOSTAT domain code (`QCL`, `FS`, `CP`).
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "QCL" => Some(Self::CropsLivestock),
            "FS" => Some(Self::FoodSecurity),
            "CP" => Some(Self::ConsumerPrices),
            _ => None,
        }
    }

    /// The FAOSTAT domain code.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::CropsLivestock => "QCL",
            Self::FoodSecurity => "FS",
            Self::ConsumerPrices => "CP",
        }
    }

    fn stem(self) -> &'static str {
        match self {
            Self::CropsLivestock => "Production_Crops_Livestock_E_All_Data_(Normalized)",
            Self::FoodSecurity => "Food_Security_Data_E_All_Data_(Normalized)",
            Self::ConsumerPrices => "ConsumerPriceIndices_E_All_Data_(Normalized)",
        }
    }

    /// The bulk zip URL for this dataset.
    #[must_use]
    pub fn url(self) -> String {
        format!("{BULK_BASE}/{}.zip", self.stem())
    }
}

/// One FAOSTAT item and element pair to select.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Selector {
    /// FAOSTAT item code, such as `1717` (cereals, primary).
    pub item: String,
    /// FAOSTAT element code, such as `5510` (production).
    pub element: String,
}

/// One observation read from the bulk file.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    /// Item code the row belongs to.
    pub item: String,
    /// Element code the row belongs to.
    pub element: String,
    /// FAOSTAT year label, e.g. `2024` or `2021-2023` for 3-year averages.
    pub year: String,
    /// Month 1-12 for monthly series, `None` for annual rows.
    pub month: Option<u32>,
    /// Unit as published.
    pub unit: String,
    /// Numeric value; for a censored value such as `<2.5` the bound (`2.5`).
    pub value: f64,
    /// `<` or `>` when FAOSTAT publishes only a bound, as for the prevalence
    /// of undernourishment below 2.5 percent.
    pub qualifier: Option<char>,
    /// FAOSTAT flag (`A` official, `E` estimated, `I` imputed, ...), may be empty.
    pub flag: String,
}

impl Observation {
    fn recency(&self) -> (i32, u32) {
        let end = self
            .year
            .rsplit('-')
            .next()
            .and_then(|part| part.trim().parse().ok())
            .unwrap_or(0);
        (end, self.month.unwrap_or(0))
    }

    /// Vintage label: the year label, with `-MM` appended for monthly rows.
    #[must_use]
    pub fn vintage(&self) -> String {
        match self.month {
            Some(month) => format!("{}-{month:02}", self.year),
            None => self.year.clone(),
        }
    }
}

/// Errors returned while fetching or decoding a FAOSTAT bulk file.
#[derive(Debug)]
#[non_exhaustive]
pub enum ClientError {
    /// The country code is not a known ISO 3166-1 alpha-3 code.
    InvalidCountry(String),
    /// The HTTP request failed.
    Http(reqwest::Error),
    /// The download exceeded the in-memory cap.
    TooLarge(u64),
    /// The zip or its CSV could not be decoded.
    Decode(String),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCountry(code) => write!(f, "unknown ISO alpha-3 country: {code}"),
            Self::Http(error) => write!(f, "FAOSTAT request failed: {error}"),
            Self::TooLarge(limit) => write!(f, "FAOSTAT bulk file exceeds {limit} bytes"),
            Self::Decode(message) => write!(f, "invalid FAOSTAT bulk file: {message}"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::InvalidCountry(_) | Self::TooLarge(_) | Self::Decode(_) => None,
        }
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

/// Returns the UN M49 numeric code for an ISO 3166-1 alpha-3 code.
#[must_use]
pub fn m49_for_iso3(iso3: &str) -> Option<u16> {
    iso::ISO3_M49
        .binary_search_by_key(&iso3, |(code, _)| code)
        .ok()
        .map(|index| iso::ISO3_M49[index].1)
}

/// Blocking client that streams a bulk zip and keeps nothing on disk.
#[derive(Clone, Debug)]
pub struct FaostatClient {
    http: HttpClient,
}

impl FaostatClient {
    /// Creates a client with a 300 second timeout.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn new() -> Result<Self, ClientError> {
        Ok(Self {
            http: HttpClient::builder()
                .user_agent(concat!("statsapi-faostat/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(300))
                .build()?,
        })
    }

    /// Downloads the dataset zip into memory and returns the rows for one
    /// country matching any selector, newest first. Nothing is written to disk.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown country, a failed or oversized
    /// download, or a malformed archive.
    pub fn fetch(
        &self,
        dataset: Dataset,
        iso3: &str,
        selectors: &[Selector],
    ) -> Result<(String, Vec<Observation>), ClientError> {
        let m49 = m49_for_iso3(iso3).ok_or_else(|| ClientError::InvalidCountry(iso3.into()))?;
        let url = dataset.url();
        let response = self.http.get(&url).send()?.error_for_status()?;
        let mut body = Vec::new();
        response
            .take(MAX_ZIP_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|error| ClientError::Decode(error.to_string()))?;
        if body.len() as u64 > MAX_ZIP_BYTES {
            return Err(ClientError::TooLarge(MAX_ZIP_BYTES));
        }
        let rows = filter_zip(&body, dataset.stem(), m49, selectors)?;
        Ok((url, rows))
    }
}

/// Filters a bulk zip held in memory to one country's rows for the selectors.
///
/// # Errors
///
/// Returns [`ClientError::Decode`] when the archive or CSV is malformed.
pub fn filter_zip(
    zip_bytes: &[u8],
    stem: &str,
    m49: u16,
    selectors: &[Selector],
) -> Result<Vec<Observation>, ClientError> {
    let decode = |error: &dyn fmt::Display| ClientError::Decode(error.to_string());
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes)).map_err(|e| decode(&e))?;
    let file = archive
        .by_name(&format!("{stem}.csv"))
        .map_err(|e| decode(&e))?;
    filter_csv(file, m49, selectors)
}

/// Filters a normalized FAOSTAT CSV stream to one country and the selectors.
///
/// # Errors
///
/// Returns [`ClientError::Decode`] when required columns are missing or a
/// record cannot be read.
pub fn filter_csv(
    reader: impl Read,
    m49: u16,
    selectors: &[Selector],
) -> Result<Vec<Observation>, ClientError> {
    let decode = |error: &dyn fmt::Display| ClientError::Decode(error.to_string());
    let mut csv = csv::ReaderBuilder::new().flexible(true).from_reader(reader);
    let headers = csv.byte_headers().map_err(|e| decode(&e))?.clone();
    let column = |name: &str| {
        headers
            .iter()
            .position(|header| header == name.as_bytes())
            .ok_or_else(|| ClientError::Decode(format!("missing column {name}")))
    };
    let area = column("Area Code (M49)")?;
    let item = column("Item Code")?;
    let element = column("Element Code")?;
    let year = column("Year")?;
    let unit = column("Unit")?;
    let value = column("Value")?;
    let flag = column("Flag")?;
    let months = column("Months Code").ok();
    let want_area = format!("{m49:03}");
    let text = |record: &csv::ByteRecord, index: usize| {
        String::from_utf8_lossy(record.get(index).unwrap_or_default()).into_owned()
    };
    let mut rows = Vec::new();
    for record in csv.byte_records() {
        let record = record.map_err(|e| decode(&e))?;
        if text(&record, area).trim_start_matches('\'') != want_area {
            continue;
        }
        let (row_item, row_element) = (text(&record, item), text(&record, element));
        if !selectors
            .iter()
            .any(|s| s.item == row_item && s.element == row_element)
        {
            continue;
        }
        let raw = text(&record, value);
        let raw = raw.trim();
        let qualifier = raw.chars().next().filter(|c| matches!(c, '<' | '>'));
        let Ok(number) = raw.trim_start_matches(['<', '>']).trim().parse::<f64>() else {
            continue;
        };
        let month = match months {
            Some(index) => {
                let code: u32 = text(&record, index).trim().parse().unwrap_or(0);
                match code {
                    7001..=7012 => Some(code - 7000),
                    7021 | 0 => None,
                    _ => continue,
                }
            }
            None => None,
        };
        rows.push(Observation {
            item: row_item,
            element: row_element,
            year: text(&record, year),
            month,
            unit: text(&record, unit),
            value: number,
            qualifier,
            flag: text(&record, flag),
        });
    }
    rows.sort_by_key(|row| std::cmp::Reverse(row.recency()));
    Ok(rows)
}

/// Returns the newest observation for a selector.
#[must_use]
pub fn latest<'a>(rows: &'a [Observation], selector: &Selector) -> Option<&'a Observation> {
    rows.iter()
        .filter(|row| row.item == selector.item && row.element == selector.element)
        .max_by_key(|row| row.recency())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/qcl-sample.csv");

    fn selector(item: &str, element: &str) -> Selector {
        Selector {
            item: item.into(),
            element: element.into(),
        }
    }

    #[test]
    fn iso_table_is_sorted_and_resolves_the_united_states() {
        assert!(iso::ISO3_M49.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert_eq!(m49_for_iso3("USA"), Some(840));
        assert_eq!(m49_for_iso3("AFG"), Some(4));
        assert_eq!(m49_for_iso3("XXX"), None);
    }

    #[test]
    fn filter_keeps_one_country_and_selected_pairs_newest_first() {
        let rows = filter_csv(FIXTURE.as_bytes(), 840, &[selector("1717", "5510")])
            .expect("fixture parses");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].year, "2024");
        assert!((rows[0].value - 400.0).abs() < f64::EPSILON);
        let newest = latest(&rows, &selector("1717", "5510")).expect("newest row");
        assert_eq!(newest.vintage(), "2024");
        assert_eq!(newest.unit, "t");
    }

    #[test]
    fn censored_values_keep_their_bound_and_qualifier() {
        let csv = "Area Code (M49),Item Code,Element Code,Year,Unit,Value,Flag\n'840,210041,6121,2021-2023,%,<2.5,E\n";
        let rows = filter_csv(csv.as_bytes(), 840, &[selector("210041", "6121")]).expect("parses");
        assert_eq!(rows[0].qualifier, Some('<'));
        assert!((rows[0].value - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn three_year_labels_order_by_their_last_year() {
        let row = Observation {
            item: "1".into(),
            element: "2".into(),
            year: "2021-2023".into(),
            month: None,
            unit: "%".into(),
            value: 1.0,
            qualifier: None,
            flag: String::new(),
        };
        assert_eq!(row.recency(), (2023, 0));
    }

    #[test]
    fn missing_columns_are_reported() {
        let error = filter_csv("a,b\n1,2\n".as_bytes(), 840, &[]).expect_err("no columns");
        assert!(error.to_string().contains("missing column"));
    }

    #[test]
    fn unknown_country_is_rejected_before_any_request() {
        let client = FaostatClient::new().expect("HTTP client builds");
        let error = client
            .fetch(Dataset::FoodSecurity, "XXX", &[])
            .expect_err("unknown country");
        assert!(matches!(error, ClientError::InvalidCountry(_)));
    }

    #[test]
    #[ignore = "live FAOSTAT bulk download (about 2 MB)"]
    fn live_food_security_returns_undernourishment_for_the_united_states() {
        let selector = selector("210041", "6121");
        let (_, rows) = FaostatClient::new()
            .expect("HTTP client builds")
            .fetch(
                Dataset::FoodSecurity,
                "USA",
                std::slice::from_ref(&selector),
            )
            .expect("FAOSTAT response");
        assert!(latest(&rows, &selector).is_some());
    }
}
