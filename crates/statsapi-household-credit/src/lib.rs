//! Reusable access to public household credit datasets from three US research institutions.

mod client;
mod parse;

pub use client::{Client, Error};
pub use parse::{county_debt_to_income_bound, ny_state_value, urban_value, CountyBound, NyMetric};

/// Urban Institute Debt in America CSV base URL.
pub const URBAN_BASE: &str =
    "https://apps.urban.org/features/debt-interactive-map/data/20261022-update";
/// New York Fed Household Debt and Credit workbook URL.
pub const NY_URL: &str = "https://www.newyorkfed.org/medialibrary/interactives/householdcredit/data/xls/hhd_c_report_2026q3.xlsx";
/// Federal Reserve Board county debt ZIP URL.
pub const NY_COUNTY_URL: &str =
    "https://www.federalreserve.gov/releases/z1/dataviz/download/zips/household-debt-by-county.zip";
