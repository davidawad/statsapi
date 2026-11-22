use statsapi_household_credit::{county_debt_to_income_bound, urban_value, CountyBound};

#[test]
fn parses_synthetic_upstream_layouts() {
    let urban = include_bytes!("fixtures/urban_middlesex.csv");
    assert_eq!(
        urban_value(urban, "county", "99999", "totcoll").expect("urban CSV parses"),
        Some((0.42, "2026-08 (published 2026-10-22)"))
    );
    let archive = include_bytes!("fixtures/fed_county_interval.zip");
    assert_eq!(
        county_debt_to_income_bound(archive, "99999", CountyBound::Low).expect("archive parses"),
        Some((0.81, "2025 Q4".into()))
    );
    assert_eq!(
        county_debt_to_income_bound(archive, "99999", CountyBound::High).expect("archive parses"),
        Some((1.07, "2025 Q4".into()))
    );
}

#[test]
fn parses_synthetic_new_york_fed_workbook() {
    use statsapi_household_credit::{ny_state_value, NyMetric};
    let workbook = include_bytes!("fixtures/ny_hhdc.xlsx");
    assert_eq!(
        ny_state_value(workbook, "ZZ", NyMetric::StateDebtPerCapita).expect("workbook parses"),
        Some((1234.5, "2026 Q3"))
    );
    assert_eq!(
        ny_state_value(workbook, "ZZ", NyMetric::StateDelinquency90Plus).expect("workbook parses"),
        Some((2.75, "2026 Q3 latest quarter"))
    );
}
