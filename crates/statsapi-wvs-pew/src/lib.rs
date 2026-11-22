//! Parses caller-provided WVS and Pew survey files and computes weighted WVS estimates.

use csv::StringRecord;
use std::error::Error as StdError;
use std::fmt;

/// A WVS measure recognized by this crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum WvsMetric {
    /// Share reporting that most people can be trusted.
    GeneralizedTrust,
    /// Share saying religion is very or rather important.
    ReligionImportance,
    /// Mean life-satisfaction response on the 1–10 scale.
    LifeSatisfaction,
    /// Share expressing confidence in government.
    GovernmentConfidence,
    /// Share reporting interest in politics.
    PoliticalInterest,
}

impl WvsMetric {
    /// Returns the adapter's stable name for this measure.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::GeneralizedTrust => "generalized_trust",
            Self::ReligionImportance => "religion_importance",
            Self::LifeSatisfaction => "life_satisfaction",
            Self::GovernmentConfidence => "government_confidence",
            Self::PoliticalInterest => "political_interest",
        }
    }

    /// Resolves a stable name or source question column to a measure.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "generalized_trust" | "Q57" => Some(Self::GeneralizedTrust),
            "religion_importance" | "Q6" => Some(Self::ReligionImportance),
            "life_satisfaction" | "Q49" => Some(Self::LifeSatisfaction),
            "government_confidence" | "Q70" | "Q71" => Some(Self::GovernmentConfidence),
            "political_interest" | "Q199" => Some(Self::PoliticalInterest),
            _ => None,
        }
    }

    const fn question(self) -> &'static str {
        match self {
            Self::GeneralizedTrust => "Q57",
            Self::ReligionImportance => "Q6",
            Self::LifeSatisfaction => "Q49",
            Self::GovernmentConfidence => "Q70",
            Self::PoliticalInterest => "Q199",
        }
    }

    const fn is_percent(self) -> bool {
        !matches!(self, Self::LifeSatisfaction)
    }
}

/// A computed WVS value and the source vintage used to calculate it.
#[derive(Clone, Debug, PartialEq)]
pub struct WvsEstimate {
    /// Weighted share in percent or weighted mean on the 1–10 scale.
    pub value: f64,
    /// Human-readable survey vintage.
    pub vintage: String,
}

/// A published Pew aggregate row selected from a caller-provided CSV.
#[derive(Clone, Debug, PartialEq)]
pub struct PewAggregate {
    /// Published numeric value.
    pub value: f64,
    /// Survey vintage label.
    pub vintage: String,
    /// Field date or `unknown` when the file omits it.
    pub date: String,
    /// Citation URL, defaulting to Pew's dataset page when omitted.
    pub source_url: String,
}

/// Failure while reading malformed delimited survey data.
#[derive(Debug)]
pub struct ParseError {
    source: csv::Error,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.source)
    }
}

impl StdError for ParseError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.source)
    }
}

impl From<csv::Error> for ParseError {
    fn from(source: csv::Error) -> Self {
        Self { source }
    }
}

/// Computes a weighted estimate from a caller-supplied WVS CSV file.
///
/// The file must use WVS-style columns or the normalized aliases documented by the caller.
/// `time_series` chooses the latest available country year and labels the returned vintage.
/// This function performs no file, network, credential, or cache access.
///
/// # Errors
/// Returns an error when CSV headers or rows are malformed.
pub fn weighted_wvs(
    bytes: &[u8],
    country: &str,
    metric: WvsMetric,
    time_series: bool,
) -> Result<Option<WvsEstimate>, ParseError> {
    let mut reader = csv::Reader::from_reader(bytes);
    let headers = reader.headers()?.clone();
    let rows = reader.records().collect::<Result<Vec<_>, _>>()?;
    let latest_year = rows
        .iter()
        .filter(|row| {
            field(
                &headers,
                row,
                &["country", "B_COUNTRY_ALPHA", "country_code"],
            )
            .is_some_and(|code| code.eq_ignore_ascii_case(country))
        })
        .filter_map(|row| {
            field(&headers, row, &["year", "S020", "survey_year"])
                .and_then(|year| year.parse::<i32>().ok())
        })
        .max();
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    for row in &rows {
        if field(
            &headers,
            row,
            &["country", "B_COUNTRY_ALPHA", "country_code"],
        )
        .is_none_or(|code| !code.eq_ignore_ascii_case(country))
        {
            continue;
        }
        if let Some(latest_year) = latest_year {
            let row_year = field(&headers, row, &["year", "S020", "survey_year"])
                .and_then(|year| year.parse::<i32>().ok());
            if row_year != Some(latest_year) {
                continue;
            }
        }
        let Some(weight) = field(&headers, row, &["weight", "S017", "S017A"])
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|weight| weight.is_finite() && *weight > 0.0)
        else {
            continue;
        };
        let max_response = match metric {
            WvsMetric::GeneralizedTrust => 2.0,
            WvsMetric::ReligionImportance
            | WvsMetric::GovernmentConfidence
            | WvsMetric::PoliticalInterest => 4.0,
            WvsMetric::LifeSatisfaction => 10.0,
        };
        let aliases: &[&str] = if metric == WvsMetric::GovernmentConfidence {
            &[metric.name(), metric.question(), "Q71"]
        } else {
            &[metric.name(), metric.question()]
        };
        let Some(value) = field(&headers, row, aliases)
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite() && (1.0..=max_response).contains(value))
        else {
            continue;
        };
        let included = match metric {
            WvsMetric::GeneralizedTrust => value < 1.5,
            WvsMetric::ReligionImportance
            | WvsMetric::GovernmentConfidence
            | WvsMetric::PoliticalInterest => value <= 2.0,
            WvsMetric::LifeSatisfaction => true,
        };
        denominator += weight;
        if included {
            numerator += weight * if metric.is_percent() { 1.0 } else { value };
        }
    }
    let vintage = if time_series {
        latest_year.map_or_else(
            || "WVS longitudinal (1981–2022)".to_owned(),
            |year| format!("WVS longitudinal, latest country survey {year}"),
        )
    } else {
        "2017–2022 Wave 7".to_owned()
    };
    Ok((denominator > 0.0).then_some(WvsEstimate {
        value: numerator / denominator * if metric.is_percent() { 100.0 } else { 1.0 },
        vintage,
    }))
}

/// Finds a published Pew aggregate in a caller-supplied CSV.
///
/// When `group` is `None`, rows labeled `all`, `overall`, or `all_adults` are selected.
/// This function performs no file, network, credential, or cache access.
///
/// # Errors
/// Returns an error when CSV headers or rows are malformed.
pub fn pew_aggregate(
    bytes: &[u8],
    country: &str,
    metric: &str,
    group: Option<&str>,
) -> Result<Option<PewAggregate>, ParseError> {
    let mut reader = csv::Reader::from_reader(bytes);
    let headers = reader.headers()?.clone();
    for result in reader.records() {
        let row = result?;
        if field(&headers, &row, &["country", "country_code"])
            .is_none_or(|code| !code.eq_ignore_ascii_case(country))
            || field(&headers, &row, &["metric", "indicator"])
                .is_none_or(|value| !value.eq_ignore_ascii_case(metric))
        {
            continue;
        }
        let row_group = field(&headers, &row, &["demographic", "group"]);
        let group_matches = match (group, row_group) {
            (Some(expected), Some(actual)) => actual.eq_ignore_ascii_case(expected),
            (Some(_), None) => false,
            (None, Some(actual)) => matches!(
                actual.to_ascii_lowercase().as_str(),
                "all" | "overall" | "all_adults"
            ),
            (None, None) => true,
        };
        if !group_matches {
            continue;
        }
        let Some(value) = field(&headers, &row, &["value", "percent", "share"])
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite())
        else {
            continue;
        };
        return Ok(Some(PewAggregate {
            value,
            vintage: field(&headers, &row, &["vintage", "survey"])
                .unwrap_or("Pew survey vintage")
                .to_owned(),
            date: field(&headers, &row, &["date", "as_of"])
                .unwrap_or("unknown")
                .to_owned(),
            source_url: field(&headers, &row, &["source_url", "url"])
                .unwrap_or("https://www.pewresearch.org/tools-and-resources/datasets/")
                .to_owned(),
        }));
    }
    Ok(None)
}

fn field<'a>(headers: &'a StringRecord, row: &'a StringRecord, names: &[&str]) -> Option<&'a str> {
    names.iter().find_map(|name| {
        headers
            .iter()
            .position(|header| header.eq_ignore_ascii_case(name))
            .and_then(|index| row.get(index))
            .filter(|value| !value.trim().is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::{pew_aggregate, weighted_wvs, WvsMetric};

    const WVS: &[u8] = include_bytes!("../tests/fixtures/wvs_wave7.csv");
    const PEW: &[u8] = include_bytes!("../tests/fixtures/pew_aggregates.csv");

    #[test]
    fn weighted_wave7_and_latest_longitudinal_year_use_synthetic_rows() {
        let trust = weighted_wvs(WVS, "AAA", WvsMetric::GeneralizedTrust, false)
            .expect("valid fixture")
            .expect("USA observations");
        assert_eq!(trust.value, 25.0, "synthetic trust weighting");
        assert_eq!(trust.vintage, "2017–2022 Wave 7");
        let satisfaction = weighted_wvs(WVS, "AAA", WvsMetric::LifeSatisfaction, false)
            .expect("valid fixture")
            .expect("USA observations");
        assert_eq!(satisfaction.value, 6.5, "synthetic weighted mean");
        let series = weighted_wvs(WVS, "AAA", WvsMetric::GeneralizedTrust, true)
            .expect("valid fixture")
            .expect("USA observations");
        assert_eq!(series.value, 25.0);
        assert_eq!(
            series.vintage,
            "WVS longitudinal, latest country survey 2020"
        );
    }

    #[test]
    fn pew_selects_overall_and_requested_group_from_synthetic_rows() {
        let overall = pew_aggregate(PEW, "AAA", "video_use", None)
            .expect("valid fixture")
            .expect("overall row");
        assert_eq!(overall.value, 37.0);
        assert_eq!(overall.vintage, "synthetic-2024");
        let group = pew_aggregate(PEW, "AAA", "video_use", Some("young_adults"))
            .expect("valid fixture")
            .expect("group row");
        assert_eq!(group.value, 61.0);
    }

    #[test]
    fn absent_values_are_not_invented() {
        assert!(weighted_wvs(WVS, "ZZZ", WvsMetric::GeneralizedTrust, false)
            .expect("valid fixture")
            .is_none());
        assert!(pew_aggregate(PEW, "ZZZ", "video_use", None)
            .expect("valid fixture")
            .is_none());
    }
}
