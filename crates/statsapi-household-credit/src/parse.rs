use calamine::{open_workbook_auto_from_rs, Data, Reader};
use std::io::Cursor;

/// Household debt metric available from the New York Fed state workbook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NyMetric {
    /// State debt per capita from the workbook's Page 33 Data sheet.
    StateDebtPerCapita,
    /// State 90+ day delinquency from the workbook's Page 35 Data sheet.
    StateDelinquency90Plus,
}

/// Lower or upper endpoint of a county debt-to-income interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountyBound {
    /// Lower endpoint.
    Low,
    /// Upper endpoint.
    High,
}

/// Reads one metric from an Urban Institute CSV in its source layout.
///
/// # Errors
/// Returns a message when required headers or CSV rows are malformed.
pub fn urban_value(
    bytes: &[u8],
    level: &str,
    geo_id: &str,
    column: &str,
) -> Result<Option<(f64, &'static str)>, String> {
    let mut reader = csv::Reader::from_reader(bytes);
    let headers = reader.headers().map_err(|error| error.to_string())?.clone();
    let geo_col = headers
        .iter()
        .position(|header| header == "id")
        .ok_or("Urban CSV missing id column")?;
    let value_col = headers
        .iter()
        .position(|header| header == column)
        .ok_or_else(|| format!("Urban CSV missing {column}"))?;
    for row in reader.records() {
        let row = row.map_err(|error| error.to_string())?;
        let key = row.get(geo_col).unwrap_or_default();
        if if level == "us" {
            key == "USA"
        } else {
            key == geo_id
        } {
            return Ok(row
                .get(value_col)
                .and_then(|value| value.parse().ok())
                .map(|value| (value, "2026-08 (published 2026-10-22)")));
        }
    }
    Ok(None)
}

/// Reads a state metric from the New York Fed quarterly workbook.
///
/// # Errors
/// Returns a message when the workbook or required worksheet cannot be parsed.
pub fn ny_state_value(
    bytes: &[u8],
    state: &str,
    metric: NyMetric,
) -> Result<Option<(f64, &'static str)>, String> {
    let mut workbook =
        open_workbook_auto_from_rs(Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let (sheet, value_col, vintage) = match metric {
        NyMetric::StateDebtPerCapita => ("Page 33 Data", 1, "2026 Q3"),
        NyMetric::StateDelinquency90Plus => ("Page 35 Data", 30, "2026 Q3 latest quarter"),
    };
    let range = workbook
        .worksheet_range(sheet)
        .map_err(|error| error.to_string())?;
    let Some((_, row)) = range
        .rows()
        .enumerate()
        .find(|(_, row)| row.first().and_then(cell_str) == Some(state))
    else {
        return Ok(None);
    };
    let value = row.get(value_col).and_then(cell_num).or_else(|| {
        if metric == NyMetric::StateDelinquency90Plus {
            row.iter().rev().find_map(cell_num)
        } else {
            None
        }
    });
    Ok(value.map(|value| (value, vintage)))
}

/// Reads the latest county interval bound from the Federal Reserve ZIP archive.
///
/// # Errors
/// Returns a message when the archive, CSV layout, or requested row is malformed.
pub fn county_debt_to_income_bound(
    archive: &[u8],
    county_fips: &str,
    bound: CountyBound,
) -> Result<Option<(f64, String)>, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(archive)).map_err(|error| error.to_string())?;
    let mut file = zip
        .by_name("household-debt-by-county.csv")
        .map_err(|error| error.to_string())?;
    let mut csv_bytes = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut csv_bytes).map_err(|error| error.to_string())?;
    let mut reader = csv::Reader::from_reader(csv_bytes.as_slice());
    let headers = reader.headers().map_err(|error| error.to_string())?.clone();
    let find = |name| {
        headers
            .iter()
            .position(|header| header == name)
            .ok_or_else(|| format!("missing {name} column"))
    };
    let year_col = find("year")?;
    let quarter_col = find("qtr")?;
    let area_col = find("area_fips")?;
    let bound_col = find(match bound {
        CountyBound::Low => "low",
        CountyBound::High => "high",
    })?;
    let mut latest: Option<(u16, u8, f64)> = None;
    for row in reader.records() {
        let row = row.map_err(|error| error.to_string())?;
        if row.get(area_col) != Some(county_fips) {
            continue;
        }
        let year = row
            .get(year_col)
            .and_then(|value| value.parse().ok())
            .ok_or("invalid year")?;
        let quarter = row
            .get(quarter_col)
            .and_then(|value| value.parse().ok())
            .ok_or("invalid quarter")?;
        let value = row
            .get(bound_col)
            .and_then(|value| value.parse().ok())
            .ok_or("invalid interval bound")?;
        if latest.is_none_or(|(old_year, old_quarter, _)| (year, quarter) > (old_year, old_quarter))
        {
            latest = Some((year, quarter, value));
        }
    }
    Ok(latest.map(|(year, quarter, value)| (value, format!("{year} Q{quarter}"))))
}

fn cell_str(value: &Data) -> Option<&str> {
    if let Data::String(text) = value {
        Some(text.as_str())
    } else {
        None
    }
}
fn cell_num(value: &Data) -> Option<f64> {
    match value {
        Data::Float(value) => Some(*value),
        Data::Int(value) => value.to_string().parse().ok(),
        _ => None,
    }
}
