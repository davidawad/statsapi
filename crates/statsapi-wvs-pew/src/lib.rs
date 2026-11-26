//! Parses caller-provided WVS and Pew survey files and computes weighted WVS estimates.

use calamine::{open_workbook_auto, Data, Reader};
use csv::StringRecord;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error as StdError;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// A WVS measure recognized by the values-survey-data adapter.
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

// ---------------------------------------------------------------------------
// Generic Pew dataset reader
// ---------------------------------------------------------------------------

const PEW_DATASET_URL: &str = "https://www.pewresearch.org/tools-and-resources/datasets/";

/// A dataset folder discovered below `PEW_DATA_DIR`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PewDatasetInfo {
    /// The folder name used in a `pew.q:<dataset>:...` definition.
    pub slug: String,
    /// The respondent data file selected from the folder.
    pub data_file: String,
    /// The dictionary or codebook selected from the folder, when present.
    pub dictionary_file: Option<String>,
}

/// A question exposed by a Pew dictionary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PewQuestion {
    /// The respondent-file column name.
    pub column: String,
    /// The human-readable question label.
    pub label: String,
    /// Code sets available for this question, with their selected codes.
    pub code_sets: BTreeMap<String, Vec<String>>,
}

/// The result of a weighted generic Pew question.
#[derive(Clone, Debug, PartialEq)]
pub struct PewQuestionEstimate {
    /// Weighted share in percent.
    pub value: f64,
    /// Dictionary question label.
    pub label: String,
    /// Dataset folder slug.
    pub dataset: String,
    /// Survey field-date range.
    pub vintage: String,
    /// Last interview date, or `unknown` when no date column is available.
    pub date: String,
    /// Pew's dataset landing page.
    pub source_url: String,
    /// Demographic group selected, if any.
    pub demographic: Option<String>,
}

/// A generic Pew reader over manually downloaded dataset folders.
#[derive(Clone, Debug)]
pub struct PewReader {
    root: PathBuf,
}

impl PewReader {
    /// Reads datasets below `root` without network access or credential lookup.
    #[must_use]
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Lists every dataset folder that has a respondent file.
    ///
    /// # Errors
    /// Returns an error when the root directory cannot be read.
    pub fn datasets(&self) -> Result<Vec<PewDatasetInfo>, GenericPewError> {
        let mut out = Vec::new();
        let entries = fs::read_dir(&self.root)
            .map_err(|error| GenericPewError(format!("{}: {error}", self.root.display())))?;
        for entry in entries {
            let entry = entry.map_err(|error| GenericPewError(error.to_string()))?;
            if !entry
                .file_type()
                .map_err(|error| GenericPewError(error.to_string()))?
                .is_dir()
            {
                continue;
            }
            let path = entry.path();
            if let Some(info) = discover_dataset(&path) {
                out.push(info);
            }
        }
        out.sort_by(|left, right| left.slug.cmp(&right.slug));
        Ok(out)
    }

    /// Lists dictionary questions in one dataset folder.
    ///
    /// # Errors
    /// Returns an error when the dataset or its dictionary cannot be read.
    pub fn questions(&self, dataset: &str) -> Result<Vec<PewQuestion>, GenericPewError> {
        let info = self.dataset(dataset)?;
        let dictionary_file = info.dictionary_file.as_deref().ok_or_else(|| {
            GenericPewError(format!(
                "Pew dataset {dataset:?} has no dictionary or codebook"
            ))
        })?;
        let dictionary = read_dictionary(Path::new(dictionary_file))?;
        Ok(dictionary
            .variables
            .values()
            .filter(|variable| variable.is_question())
            .map(Variable::question)
            .collect())
    }

    /// Answers one question for one ISO3 country and optional demographic group.
    ///
    /// # Errors
    /// Returns an error when the dataset, dictionary, or requested question is malformed.
    pub fn query(
        &self,
        dataset: &str,
        column: &str,
        code_set: Option<&str>,
        country: &str,
        demographic: Option<&str>,
    ) -> Result<Option<PewQuestionEstimate>, GenericPewError> {
        let info = self.dataset(dataset)?;
        query_dataset(&info, column, code_set, country, demographic)
    }

    /// Finds the first dataset containing `column`, for legacy aliases whose
    /// original definitions predate the `pew.q:` form.
    ///
    /// # Errors
    /// Returns an error when the root directory or a matching dataset is malformed.
    pub fn query_any_dataset(
        &self,
        column: &str,
        code_set: Option<&str>,
        country: &str,
        demographic: Option<&str>,
    ) -> Result<Option<PewQuestionEstimate>, GenericPewError> {
        for info in self.datasets()? {
            if let Some(estimate) = query_dataset(&info, column, code_set, country, demographic)? {
                return Ok(Some(estimate));
            }
        }
        Ok(None)
    }

    fn dataset(&self, slug: &str) -> Result<PewDatasetInfo, GenericPewError> {
        let path = self.root.join(slug);
        discover_dataset(&path).ok_or_else(|| {
            GenericPewError(format!("Pew dataset {slug:?} has no CSV/SAV data file"))
        })
    }
}

/// Failure while discovering or reading a generic Pew dataset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenericPewError(String);

impl GenericPewError {
    /// Creates a descriptive reader error for adapter callers.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for GenericPewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl StdError for GenericPewError {}

/// Lists dataset folders as JSON for the `pew datasets` discovery command.
///
/// # Errors
/// Returns an error when the root directory or a dataset folder cannot be read.
pub fn pew_datasets(root: impl AsRef<Path>) -> Result<Value, GenericPewError> {
    let datasets = PewReader::new(root).datasets()?;
    Ok(Value::Array(
        datasets
            .into_iter()
            .map(|dataset| {
                serde_json::json!({
                    "dataset": dataset.slug,
                    "data_file": dataset.data_file,
                    "dictionary_file": dataset.dictionary_file,
                })
            })
            .collect(),
    ))
}

/// Lists dictionary questions as JSON for `pew questions <dataset>`.
///
/// # Errors
/// Returns an error when the dataset or dictionary cannot be read.
pub fn pew_questions(root: impl AsRef<Path>, dataset: &str) -> Result<Value, GenericPewError> {
    let questions = PewReader::new(root).questions(dataset)?;
    Ok(Value::Array(
        questions
            .into_iter()
            .map(|question| {
                serde_json::json!({
                    "column": question.column,
                    "label": question.label,
                    "code_sets": question.code_sets,
                })
            })
            .collect(),
    ))
}

#[derive(Clone, Debug, Default)]
struct Dictionary {
    variables: BTreeMap<String, Variable>,
}

#[derive(Clone, Debug, Default)]
struct Variable {
    name: String,
    label: String,
    role: String,
    code_sets: BTreeMap<String, BTreeSet<String>>,
    value_labels: BTreeMap<String, String>,
}

impl Variable {
    fn is_question(&self) -> bool {
        !matches!(
            self.role.as_str(),
            "country" | "weight" | "date" | "demographic"
        )
    }

    fn question(&self) -> PewQuestion {
        PewQuestion {
            column: self.name.clone(),
            label: self.label.clone(),
            code_sets: self
                .code_sets
                .iter()
                .map(|(name, codes)| (name.clone(), codes.iter().cloned().collect()))
                .collect(),
        }
    }
}

#[derive(Clone, Debug)]
struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

#[derive(Clone, Debug)]
struct DataTable {
    table: Table,
    value_labels: BTreeMap<String, BTreeMap<String, String>>,
}

fn discover_dataset(path: &Path) -> Option<PewDatasetInfo> {
    let files = files_in(path).ok()?;
    let is_data_file = |file: &&PathBuf| {
        matches!(
            file.extension().and_then(|extension| extension.to_str()),
            Some("csv" | "sav")
        ) && file.file_stem().and_then(|stem| stem.to_str()) != Some("aggregates")
            && !file
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    let name = name.to_ascii_lowercase();
                    name.contains("dict") || name.contains("codebook") || name.contains("label")
                })
    };
    let data = files
        .iter()
        .filter(is_data_file)
        .find(|file| file.extension().and_then(|extension| extension.to_str()) == Some("sav"))
        .or_else(|| files.iter().find(is_data_file))?;
    let dictionary = files.iter().find(|file| {
        matches!(
            file.extension().and_then(|extension| extension.to_str()),
            Some("csv" | "tsv" | "json" | "xlsx" | "xls")
        ) && *file != data
            && file
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    let name = name.to_ascii_lowercase();
                    name.contains("dict") || name.contains("codebook") || name.contains("label")
                })
    });
    Some(PewDatasetInfo {
        slug: path.file_name()?.to_string_lossy().into_owned(),
        data_file: data.to_string_lossy().into_owned(),
        dictionary_file: dictionary.map(|file| file.to_string_lossy().into_owned()),
    })
}

fn files_in(path: &Path) -> Result<Vec<PathBuf>, GenericPewError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(path).map_err(|error| GenericPewError(error.to_string()))? {
        let entry = entry.map_err(|error| GenericPewError(error.to_string()))?;
        if entry
            .file_type()
            .map_err(|error| GenericPewError(error.to_string()))?
            .is_file()
        {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

#[expect(
    clippy::too_many_lines,
    reason = "the query keeps file, dictionary, filter, weighting, and provenance steps together"
)]
fn query_dataset(
    info: &PewDatasetInfo,
    column: &str,
    code_set: Option<&str>,
    country: &str,
    demographic: Option<&str>,
) -> Result<Option<PewQuestionEstimate>, GenericPewError> {
    let data_table = read_data(Path::new(&info.data_file))?;
    let mut dictionary = info
        .dictionary_file
        .as_deref()
        .map(Path::new)
        .map(read_dictionary)
        .transpose()?
        .unwrap_or_default();
    for header in &data_table.table.headers {
        dictionary
            .variables
            .entry(header.to_ascii_lowercase())
            .or_insert_with(|| Variable {
                name: header.clone(),
                label: header.clone(),
                ..Variable::default()
            });
    }
    for (name, labels) in data_table.value_labels {
        let variable = dictionary
            .variables
            .entry(name.to_ascii_lowercase())
            .or_insert_with(|| Variable {
                name: name.clone(),
                label: name.clone(),
                ..Variable::default()
            });
        variable.value_labels.extend(labels);
    }
    mark_roles(&mut dictionary);
    let variable = dictionary
        .variables
        .values()
        .find(|variable| variable.name.eq_ignore_ascii_case(column))
        .ok_or_else(|| {
            GenericPewError(format!(
                "question column {column:?} is absent from dictionary"
            ))
        })?;
    let table = data_table.table;
    let question_index = index_of(&table.headers, &variable.name).ok_or_else(|| {
        GenericPewError(format!("question column {column:?} is absent from data"))
    })?;
    let country_variable = dictionary
        .variables
        .values()
        .filter(|variable| variable.role == "country")
        .min_by_key(|variable| !is_country_name(&variable.name))
        .or_else(|| {
            table
                .headers
                .iter()
                .find(|header| is_country_name(header))
                .and_then(|header| dictionary.variables.get(header))
        });
    let country_header = country_variable
        .map(|variable| variable.name.as_str())
        .or_else(|| {
            table
                .headers
                .iter()
                .find(|header| is_country_name(header))
                .map(String::as_str)
        })
        .ok_or_else(|| GenericPewError("dictionary has no country column".to_owned()))?;
    let country_index = index_of(&table.headers, country_header)
        .ok_or_else(|| GenericPewError("country column is absent from data".to_owned()))?;
    let weight_header = dictionary
        .variables
        .values()
        .find(|variable| variable.role == "weight")
        .map(|variable| variable.name.as_str())
        .or_else(|| {
            table
                .headers
                .iter()
                .find(|header| is_weight_name(header))
                .map(String::as_str)
        })
        .ok_or_else(|| GenericPewError("dictionary has no weight column".to_owned()))?;
    let weight_index = index_of(&table.headers, weight_header)
        .ok_or_else(|| GenericPewError("weight column is absent from data".to_owned()))?;
    let selected_codes = selected_codes(variable, code_set);
    if selected_codes.is_empty() {
        return Err(GenericPewError(format!(
            "question {column:?} has no codes for code-set {:?}",
            code_set.unwrap_or("default")
        )));
    }
    let group_filter = demographic.and_then(|group| demographic_filter(group, &dictionary, &table));
    if demographic.is_some() && group_filter.is_none() {
        return Ok(None);
    }
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    let mut dates = Vec::new();
    for row in &table.rows {
        if !country_matches(
            row.get(country_index).map_or("", String::as_str),
            country,
            country_variable,
        ) {
            continue;
        }
        if let Some((index, expected)) = &group_filter {
            if !value_matches(
                row.get(*index).map_or("", String::as_str),
                expected,
                dictionary_variable(&dictionary, &table.headers[*index]),
            ) {
                continue;
            }
        }
        let Some(weight) = row
            .get(weight_index)
            .and_then(|value| value.trim().parse::<f64>().ok())
            .filter(|weight| weight.is_finite() && *weight > 0.0)
        else {
            continue;
        };
        let Some(response) = row
            .get(question_index)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if is_not_asked_response(response, variable) {
            continue;
        }
        denominator += weight;
        if selected_codes.contains(&code_key(response)) {
            numerator += weight;
        }
        for header in &table.headers {
            if is_date_name(header) {
                if let Some(index) = index_of(&table.headers, header) {
                    if let Some(date) = normalize_date(row.get(index).map_or("", String::as_str)) {
                        dates.push(date);
                    }
                }
            }
        }
    }
    if denominator <= 0.0 {
        return Ok(None);
    }
    dates.sort();
    let (vintage, last_date) = match (dates.first(), dates.last()) {
        (Some(first), Some(last)) if first == last => {
            (format!("{}, fielded {first}", info.slug), last.clone())
        }
        (Some(first), Some(last)) => (
            format!("{}, fielded {first} to {last}", info.slug),
            last.clone(),
        ),
        _ => (format!("{} survey", info.slug), "unknown".to_owned()),
    };
    Ok(Some(PewQuestionEstimate {
        value: numerator * 100.0 / denominator,
        label: variable.label.clone(),
        dataset: info.slug.clone(),
        vintage,
        date: last_date,
        source_url: PEW_DATASET_URL.to_owned(),
        demographic: demographic.map(str::to_owned),
    }))
}

fn read_data(path: &Path) -> Result<DataTable, GenericPewError> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("csv") => read_csv(path, b',')
            .or_else(|_| read_csv(path, b'\t'))
            .map(|table| DataTable {
                table,
                value_labels: BTreeMap::new(),
            }),
        Some("sav") => read_sav(path),
        _ => Err(GenericPewError(format!(
            "unsupported Pew data file {}",
            path.display()
        ))),
    }
}

fn read_csv(path: &Path, delimiter: u8) -> Result<Table, GenericPewError> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .delimiter(delimiter)
        .from_path(path)
        .map_err(|error| GenericPewError(error.to_string()))?;
    let headers = reader
        .headers()
        .map_err(|error| GenericPewError(error.to_string()))?
        .iter()
        .map(|header| header.trim_start_matches('\u{feff}').trim().to_owned())
        .collect();
    let rows = reader
        .records()
        .map(|record| {
            record
                .map(|record| record.iter().map(str::to_owned).collect())
                .map_err(|error| GenericPewError(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Table { headers, rows })
}

fn read_sav(path: &Path) -> Result<DataTable, GenericPewError> {
    let (batch, metadata) = ambers::read_sav(path)
        .map_err(|error| GenericPewError(format!("{}: {error}", path.display())))?;
    let headers = metadata.variable_names;
    let mut rows = Vec::with_capacity(batch.num_rows());
    for row_index in 0..batch.num_rows() {
        let mut row = Vec::with_capacity(batch.num_columns());
        for column in batch.columns() {
            let value = arrow::util::display::array_value_to_string(column, row_index)
                .map_err(|error| GenericPewError(error.to_string()))?;
            row.push(value);
        }
        rows.push(row);
    }
    let value_labels = metadata
        .variable_value_labels
        .into_iter()
        .map(|(name, labels)| {
            (
                name,
                labels
                    .into_iter()
                    .map(|(value, label)| (code_key(&value.to_string()), label))
                    .collect(),
            )
        })
        .collect();
    Ok(DataTable {
        table: Table { headers, rows },
        value_labels,
    })
}

fn read_dictionary(path: &Path) -> Result<Dictionary, GenericPewError> {
    let rows = match path.extension().and_then(|extension| extension.to_str()) {
        Some("xlsx" | "xls") => spreadsheet_rows(path)?,
        Some("json") => return json_dictionary(path),
        Some("csv") => read_csv(path, b',').map(|table| table_to_rows(&table))?,
        Some("tsv") => read_csv(path, b'\t').map(|table| table_to_rows(&table))?,
        _ => {
            return Err(GenericPewError(format!(
                "unsupported Pew dictionary {}",
                path.display()
            )))
        }
    };
    dictionary_from_rows(&rows)
}

fn table_to_rows(table: &Table) -> Vec<Vec<String>> {
    let mut rows = Vec::with_capacity(table.rows.len() + 1);
    rows.push(table.headers.clone());
    rows.extend(table.rows.clone());
    rows
}

fn spreadsheet_rows(path: &Path) -> Result<Vec<Vec<String>>, GenericPewError> {
    let mut workbook =
        open_workbook_auto(path).map_err(|error| GenericPewError(error.to_string()))?;
    let mut worksheets = workbook.worksheets();
    worksheets.sort_by_key(|(name, _)| match normalize(name).as_str() {
        "variables" => 0,
        "values" => 1,
        _ => 2,
    });
    let mut rows = Vec::new();
    for (_, range) in worksheets {
        if !rows.is_empty() {
            rows.push(Vec::new());
        }
        rows.extend(range.rows().map(|row| {
            row.iter()
                .map(|cell| match cell {
                    Data::Empty => String::new(),
                    Data::Float(value) => value.to_string(),
                    Data::Int(value) => value.to_string(),
                    Data::Bool(value) => value.to_string(),
                    Data::String(value) | Data::DateTimeIso(value) | Data::DurationIso(value) => {
                        value.clone()
                    }
                    Data::Error(error) => error.to_string(),
                    Data::DateTime(value) => value.to_string(),
                })
                .collect()
        }));
    }
    Ok(rows)
}

fn json_dictionary(path: &Path) -> Result<Dictionary, GenericPewError> {
    let value: Value = serde_json::from_slice(
        &fs::read(path).map_err(|error| GenericPewError(error.to_string()))?,
    )
    .map_err(|error| GenericPewError(error.to_string()))?;
    let variables = value
        .get("variables")
        .and_then(Value::as_array)
        .ok_or_else(|| GenericPewError("Pew dictionary JSON needs a variables array".to_owned()))?;
    let mut dictionary = Dictionary::default();
    for item in variables {
        let Some(name) = item
            .get("name")
            .or_else(|| item.get("column"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let variable = dictionary
            .variables
            .entry(name.to_ascii_lowercase())
            .or_insert_with(|| Variable {
                name: name.to_owned(),
                label: item
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or(name)
                    .to_owned(),
                role: item
                    .get("role")
                    .and_then(Value::as_str)
                    .map_or_else(String::new, normalize),
                ..Variable::default()
            });
        if let Some(codes) = item.get("codes").and_then(Value::as_object) {
            for (code, label) in codes {
                variable
                    .value_labels
                    .insert(code_key(code), label.as_str().unwrap_or("").to_owned());
            }
        }
        if let Some(sets) = item.get("code_sets").and_then(Value::as_object) {
            for (set, codes) in sets {
                variable.code_sets.insert(
                    normalize(set),
                    codes
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(value_string)
                        .map(|code| code_key(&code))
                        .collect(),
                );
            }
        }
    }
    Ok(dictionary)
}

#[expect(
    clippy::too_many_lines,
    reason = "dictionary tables have several supported layouts and are normalized in one pass"
)]
fn dictionary_from_rows(rows: &[Vec<String>]) -> Result<Dictionary, GenericPewError> {
    let Some(header_row) = rows
        .iter()
        .position(|row| row.iter().any(|cell| is_variable_header(cell)))
    else {
        return Err(GenericPewError(
            "Pew dictionary has no variable/name column".to_owned(),
        ));
    };
    let headers = rows[header_row]
        .iter()
        .map(|value| normalize(value))
        .collect::<Vec<_>>();
    let variable_index = find_header(
        &headers,
        &[
            "variable",
            "variable_name",
            "varname",
            "name",
            "column",
            "field",
        ],
    );
    let label_index = find_header(
        &headers,
        &["variable_label", "question", "description", "label", "text"],
    );
    let role_index = find_header(&headers, &["role", "type", "variable_type"]);
    let code_index = find_header(
        &headers,
        &["code", "value", "numeric_code", "category_code"],
    );
    let code_label_index = find_header(
        &headers,
        &[
            "code_label",
            "value_label",
            "category_label",
            "response",
            "category",
        ],
    );
    let set_index = find_header(
        &headers,
        &["code_set", "codeset", "set", "scale", "category_set"],
    );
    let values_index = find_header(&headers, &["codes", "values", "value_labels", "categories"]);
    let variable_index = variable_index
        .ok_or_else(|| GenericPewError("Pew dictionary has no variable/name column".to_owned()))?;
    let mut dictionary = Dictionary::default();
    for row in rows
        .iter()
        .skip(header_row + 1)
        .take_while(|row| !row.is_empty())
    {
        let Some(name) = row
            .get(variable_index)
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
        else {
            continue;
        };
        let key = name.to_ascii_lowercase();
        let variable = dictionary.variables.entry(key).or_insert_with(|| Variable {
            name: name.trim().to_owned(),
            label: label_index
                .and_then(|index| row.get(index))
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| name.trim().to_owned()),
            role: role_index
                .and_then(|index| row.get(index))
                .map_or_else(String::new, |value| normalize(value)),
            ..Variable::default()
        });
        if variable.label == variable.name {
            if let Some(label) = label_index
                .and_then(|index| row.get(index))
                .filter(|value| !value.trim().is_empty())
            {
                variable.label.clone_from(label);
            }
        }
        if variable.role.is_empty() {
            variable.role = role_index
                .and_then(|index| row.get(index))
                .map_or_else(String::new, |value| normalize(value));
        }
        if let (Some(code_index), Some(label_index)) = (code_index, code_label_index) {
            if let (Some(code), Some(label)) = (row.get(code_index), row.get(label_index)) {
                if !code.trim().is_empty() && !label.trim().is_empty() {
                    variable
                        .value_labels
                        .insert(code_key(code), label.trim().to_owned());
                    let set = set_index
                        .and_then(|index| row.get(index))
                        .filter(|value| !value.trim().is_empty())
                        .map_or_else(|| "default".to_owned(), |value| normalize(value));
                    variable
                        .code_sets
                        .entry(set)
                        .or_default()
                        .insert(code_key(code));
                }
            }
        }
        if let Some(index) = values_index.and_then(|index| row.get(index)) {
            for (code, label) in parse_value_labels(index) {
                variable.value_labels.insert(code.clone(), label);
                variable
                    .code_sets
                    .entry("default".to_owned())
                    .or_default()
                    .insert(code);
            }
        }
    }
    for position in rows
        .iter()
        .enumerate()
        .filter_map(|(position, row)| is_values_header(row).then_some(position))
    {
        let mut current_variable = None;
        for row in rows
            .iter()
            .skip(position + 1)
            .take_while(|row| !row.is_empty())
        {
            if let Some(name) = row
                .first()
                .map(String::as_str)
                .filter(|name| !name.trim().is_empty())
            {
                current_variable = Some(name.trim().to_owned());
            }
            let Some(variable_name) = current_variable.as_deref() else {
                continue;
            };
            let (Some(code), Some(label)) = (row.get(1), row.get(2)) else {
                continue;
            };
            if code.trim().is_empty() || label.trim().is_empty() {
                continue;
            }
            let variable = dictionary
                .variables
                .entry(variable_name.to_ascii_lowercase())
                .or_insert_with(|| Variable {
                    name: variable_name.to_owned(),
                    label: variable_name.to_owned(),
                    ..Variable::default()
                });
            variable
                .value_labels
                .insert(code_key(code), label.trim().to_owned());
            variable
                .code_sets
                .entry("default".to_owned())
                .or_default()
                .insert(code_key(code));
        }
    }
    mark_roles(&mut dictionary);
    Ok(dictionary)
}

fn mark_roles(dictionary: &mut Dictionary) {
    for variable in dictionary.variables.values_mut() {
        let name = normalize(&variable.name);
        let role = normalize(&variable.role);
        // Exact names only: Pew questions such as `aireg_country` or
        // `polsys_country` sort before `country` and must not take its role.
        if name == "weight" || role == "weight" {
            "weight".clone_into(&mut variable.role);
        } else if is_country_name(&variable.name) || role == "country" {
            "country".clone_into(&mut variable.role);
        } else if is_date_name(&variable.name) || role.contains("date") {
            "date".clone_into(&mut variable.role);
        } else if role.contains("demographic")
            || name.contains("gender")
            || name.contains("age")
            || name.contains("group")
        {
            "demographic".clone_into(&mut variable.role);
        }
    }
}

fn selected_codes(variable: &Variable, requested: Option<&str>) -> BTreeSet<String> {
    if let Some(requested) = requested {
        let normalized = normalize(requested);
        if let Some(codes) = variable.code_sets.get(&normalized) {
            return codes.clone();
        }
        let parsed = parse_code_set(requested);
        if !parsed.is_empty() {
            return parsed;
        }
    }
    for name in [
        "positive",
        "favorable",
        "satisfied",
        "good",
        "yes",
        "default",
    ] {
        if let Some(codes) = variable.code_sets.get(name) {
            if name != "default" || variable.code_sets.len() == 1 {
                return codes.clone();
            }
        }
    }
    let labeled = variable
        .value_labels
        .iter()
        .filter(|(_, label)| is_positive_label(label))
        .map(|(code, _)| code.clone())
        .collect::<BTreeSet<_>>();
    if !labeled.is_empty() {
        return labeled;
    }
    let numeric = ["1".to_owned(), "2".to_owned()]
        .into_iter()
        .filter(|code| variable.value_labels.contains_key(code))
        .collect::<BTreeSet<_>>();
    if !numeric.is_empty() {
        return numeric;
    }
    variable
        .value_labels
        .keys()
        .next()
        .cloned()
        .into_iter()
        .collect()
}

fn parse_code_set(value: &str) -> BTreeSet<String> {
    value
        .split([',', '|', '+'])
        .flat_map(|part| {
            let part = part.trim();
            if let Some((start, end)) = part.split_once('-') {
                let (Ok(start), Ok(end)) = (start.trim().parse::<i64>(), end.trim().parse::<i64>())
                else {
                    return Vec::new();
                };
                (start..=end).map(|value| value.to_string()).collect()
            } else if part.is_empty() {
                Vec::new()
            } else {
                vec![code_key(part)]
            }
        })
        .collect()
}

fn demographic_filter(
    group: &str,
    dictionary: &Dictionary,
    table: &Table,
) -> Option<(usize, String)> {
    if let Some((column, value)) = group.split_once('=') {
        let index = index_of(&table.headers, column)?;
        return Some((index, value.trim().to_owned()));
    }
    dictionary
        .variables
        .values()
        .filter(|variable| variable.role == "demographic")
        .find_map(|variable| {
            let index = index_of(&table.headers, &variable.name)?;
            Some((index, group.to_owned()))
        })
}

fn dictionary_variable<'a>(dictionary: &'a Dictionary, header: &str) -> Option<&'a Variable> {
    dictionary
        .variables
        .values()
        .find(|variable| variable.name.eq_ignore_ascii_case(header))
}

fn value_matches(value: &str, expected: &str, variable: Option<&Variable>) -> bool {
    if value.eq_ignore_ascii_case(expected) || normalize(value) == normalize(expected) {
        return true;
    }
    variable.is_some_and(|variable| {
        variable
            .value_labels
            .get(&code_key(value))
            .is_some_and(|label| {
                normalize(label) == normalize(expected)
                    || normalize(label).contains(&normalize(expected))
            })
    })
}

fn country_matches(value: &str, country: &str, variable: Option<&Variable>) -> bool {
    let wanted = country.to_ascii_uppercase();
    iso3(value).is_some_and(|value| value == wanted)
        || variable.is_some_and(|variable| {
            variable
                .value_labels
                .get(&code_key(value))
                .and_then(|label| iso3(label))
                .is_some_and(|value| value == wanted)
        })
}

fn iso3(value: &str) -> Option<String> {
    let value = value.trim();
    if value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return Some(value.to_ascii_uppercase());
    }
    let normalized = normalize(value);
    isocountry::CountryCode::iter()
        .find(|code| normalize(code.name()) == normalized)
        .map(|code| code.alpha3().to_owned())
        .or_else(|| match normalized.as_str() {
            "usa" | "unitedstates" => Some("USA".to_owned()),
            "uk" | "unitedkingdom" => Some("GBR".to_owned()),
            "korea" | "southkorea" => Some("KOR".to_owned()),
            "russia" | "russianfederation" => Some("RUS".to_owned()),
            _ => None,
        })
}

fn index_of(headers: &[String], wanted: &str) -> Option<usize> {
    headers
        .iter()
        .position(|header| header.eq_ignore_ascii_case(wanted))
}

fn find_header(headers: &[String], names: &[&str]) -> Option<usize> {
    names
        .iter()
        .find_map(|name| headers.iter().position(|header| header == name))
}

fn is_variable_header(value: &str) -> bool {
    matches!(
        normalize(value).as_str(),
        "variable" | "variable_name" | "varname" | "name" | "column" | "field"
    )
}

fn is_values_header(row: &[String]) -> bool {
    row.first()
        .is_some_and(|cell| matches!(normalize(cell).as_str(), "variable" | "value"))
        && row.get(1).is_some_and(|cell| cell.trim().is_empty())
        && row.get(2).is_some_and(|cell| normalize(cell) == "label")
}

fn is_country_name(value: &str) -> bool {
    matches!(
        normalize(value).as_str(),
        "country" | "country_code" | "countrycode" | "iso3" | "country_iso3"
    )
}

fn is_weight_name(value: &str) -> bool {
    normalize(value).contains("weight")
}

fn is_date_name(value: &str) -> bool {
    let value = normalize(value);
    value.contains("date")
        || value.contains("field")
        || value.starts_with("qdate")
        || value.starts_with("intdate")
}

fn normalize(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_owned()
}

fn code_key(value: &str) -> String {
    let value = value.trim();
    value.parse::<f64>().map_or_else(
        |_| value.to_ascii_lowercase(),
        |number| {
            if number.fract() == 0.0 {
                format!("{number:.0}")
            } else {
                number.to_string()
            }
        },
    )
}

fn parse_value_labels(value: &str) -> Vec<(String, String)> {
    value
        .split([';', '|', ','])
        .filter_map(|part| {
            let (code, label) = part.split_once('=').or_else(|| part.split_once(':'))?;
            Some((code_key(code), label.trim().to_owned()))
        })
        .collect()
}

fn value_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn is_positive_label(value: &str) -> bool {
    let value = normalize(value);
    [
        "yes",
        "favorable",
        "good",
        "satisfied",
        "approve",
        "positive",
        "trust",
    ]
    .iter()
    .any(|word| value.contains(word))
}

/// Pew's published shares keep Don't know / Refused in the base; only a
/// response labeled as not asked or missing (a structural skip) leaves it.
fn is_not_asked_label(value: &str) -> bool {
    let value = normalize(value);
    ["not_asked", "missing", "skipped", "not_applicable"]
        .iter()
        .any(|word| value.contains(word))
}

fn is_not_asked_response(response: &str, variable: &Variable) -> bool {
    variable
        .value_labels
        .get(&code_key(response))
        .is_some_and(|label| is_not_asked_label(label))
}

fn normalize_date(value: &str) -> Option<String> {
    let value = value.trim();
    let pieces: Vec<&str> = value.split(['/', '-']).collect();
    if pieces.len() != 3 {
        return None;
    }
    if pieces[0].len() == 4 {
        return Some(format!(
            "{:04}-{:02}-{:02}",
            pieces[0].parse::<u32>().ok()?,
            pieces[1].parse::<u32>().ok()?,
            pieces[2].parse::<u32>().ok()?
        ));
    }
    Some(format!(
        "{:04}-{:02}-{:02}",
        pieces[2].parse::<u32>().ok()?,
        pieces[0].parse::<u32>().ok()?,
        pieces[1].parse::<u32>().ok()?
    ))
}

#[cfg(test)]
mod tests {
    use super::{pew_aggregate, weighted_wvs, PewReader, WvsMetric};
    use std::fs;

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

    #[test]
    fn generic_reader_weights_dictionary_code_set_and_dates() {
        let root =
            std::env::temp_dir().join(format!("statsapi-wvs-pew-generic-{}", std::process::id()));
        let dataset = root.join("fixture-survey");
        fs::create_dir_all(&dataset).expect("fixture directory");
        fs::write(
            dataset.join("respondents.csv"),
            "country,weight,gender,qdate_s,qdate_e,q1\nUSA,1,women,01/01/2025,01/02/2025,1\nUSA,3,women,01/03/2025,01/04/2025,0\nUSA,2,men,01/05/2025,01/06/2025,1\nCAN,4,women,01/01/2025,01/02/2025,1\n",
        )
        .expect("fixture data");
        fs::write(
            dataset.join("codebook.csv"),
            "variable,label,role,code_set,code,code_label\ncountry,Country,country,,,\nweight,Weight,weight,,,\ngender,Gender,demographic,,,\nqdate_s,Interview start,date,,,\nqdate_e,Interview end,date,,,\nq1,Would you support this?,question,positive,1,Yes\nq1,Would you support this?,question,all,0,No\n",
        )
        .expect("fixture dictionary");

        let reader = PewReader::new(&root);
        let datasets = reader.datasets().expect("datasets");
        assert_eq!(datasets.len(), 1);
        let questions = reader.questions("fixture-survey").expect("questions");
        assert_eq!(questions[0].label, "Would you support this?");
        let estimate = reader
            .query(
                "fixture-survey",
                "q1",
                Some("positive"),
                "USA",
                Some("gender=women"),
            )
            .expect("query")
            .expect("USA estimate");
        assert_eq!(estimate.value, 25.0);
        assert_eq!(estimate.date, "2025-01-04");
        assert_eq!(
            estimate.vintage,
            "fixture-survey, fielded 2025-01-01 to 2025-01-04"
        );

        fs::remove_dir_all(root).expect("remove fixture directory");
    }

    #[test]
    fn generic_reader_uses_ambers_for_sav_respondents() {
        use arrow::array::{Float64Array, StringArray};
        use arrow::datatypes::{DataType, Field, Schema};
        use arrow::record_batch::RecordBatch;
        use std::sync::Arc;

        let root =
            std::env::temp_dir().join(format!("statsapi-wvs-pew-sav-{}", std::process::id()));
        let dataset = root.join("fixture-survey");
        fs::create_dir_all(&dataset).expect("fixture directory");
        let schema = Arc::new(Schema::new(vec![
            Field::new("country", DataType::Utf8, false),
            Field::new("weight", DataType::Float64, false),
            Field::new("gender", DataType::Utf8, false),
            Field::new("q1", DataType::Float64, false),
        ]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(StringArray::from(vec!["USA", "USA"])),
                Arc::new(Float64Array::from(vec![1.0, 3.0])),
                Arc::new(StringArray::from(vec!["women", "women"])),
                Arc::new(Float64Array::from(vec![1.0, 0.0])),
            ],
        )
        .expect("valid SAV fixture batch");
        let metadata = ambers::SpssMetadata::from_arrow_schema(&schema);
        ambers::write_sav(
            dataset.join("respondents.sav"),
            &batch,
            &metadata,
            ambers::Compression::None,
            None,
        )
        .expect("write SAV fixture");
        fs::write(
            dataset.join("codebook.csv"),
            "variable,label,role,code_set,code,code_label\ncountry,Country,country,,,\nweight,Weight,weight,,,\ngender,Gender,demographic,,,\nq1,Support this?,question,positive,1,Yes\nq1,Support this?,question,all,0,No\n",
        )
        .expect("fixture dictionary");

        let reader = PewReader::new(&root);
        let estimate = reader
            .query(
                "fixture-survey",
                "q1",
                Some("positive"),
                "USA",
                Some("gender=women"),
            )
            .expect("SAV query")
            .expect("weighted estimate");
        assert_eq!(estimate.value, 25.0, "ambers SAV values preserve weights");
        assert_eq!(
            reader.questions("fixture-survey").expect("questions")[0].label,
            "Support this?"
        );

        fs::remove_dir_all(root).expect("remove fixture directory");
    }

    #[test]
    fn generic_reader_uses_ambers_value_labels_without_dictionary() {
        use arrow::array::Float64Array;
        use arrow::datatypes::{DataType, Field, Schema};
        use arrow::record_batch::RecordBatch;
        use std::sync::Arc;

        let root =
            std::env::temp_dir().join(format!("statsapi-wvs-pew-sav-only-{}", std::process::id()));
        let dataset = root.join("fixture-survey");
        fs::create_dir_all(&dataset).expect("fixture directory");
        let schema = Arc::new(Schema::new(vec![
            Field::new("country", DataType::Float64, false),
            Field::new("weight", DataType::Float64, false),
            Field::new("fav_us", DataType::Float64, false),
        ]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(Float64Array::from(vec![3.0, 3.0, 3.0, 2.0])),
                Arc::new(Float64Array::from(vec![2.0, 1.63, 3.0, 4.0])),
                Arc::new(Float64Array::from(vec![1.0, 2.0, 3.0, 1.0])),
            ],
        )
        .expect("valid SAV fixture batch");
        let mut metadata = ambers::SpssMetadata::from_arrow_schema(&schema);
        metadata.variable_value_labels.insert(
            "country".to_owned(),
            [
                (ambers::Value::Numeric(2.0), "Canada".to_owned()),
                (ambers::Value::Numeric(3.0), "France".to_owned()),
            ]
            .into_iter()
            .collect(),
        );
        metadata.variable_value_labels.insert(
            "fav_us".to_owned(),
            [
                (ambers::Value::Numeric(1.0), "Very favorable".to_owned()),
                (ambers::Value::Numeric(2.0), "Mostly favorable".to_owned()),
                (ambers::Value::Numeric(3.0), "Mostly unfavorable".to_owned()),
            ]
            .into_iter()
            .collect(),
        );
        ambers::write_sav(
            dataset.join("respondents.sav"),
            &batch,
            &metadata,
            ambers::Compression::None,
            None,
        )
        .expect("write SAV fixture");

        let reader = PewReader::new(&root);
        let datasets = reader.datasets().expect("datasets");
        assert_eq!(datasets.len(), 1);
        assert_eq!(datasets[0].dictionary_file, None);
        let estimate = reader
            .query("fixture-survey", "fav_us", Some("1,2"), "FRA", None)
            .expect("SAV-only query")
            .expect("weighted estimate");
        assert!((estimate.value - (3.63 * 100.0 / 6.63)).abs() < 1e-10);

        fs::remove_dir_all(root).expect("remove fixture directory");
    }
}
