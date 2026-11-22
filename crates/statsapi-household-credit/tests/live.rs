use statsapi_household_credit::{county_debt_to_income_bound, Client, CountyBound};

#[test]
#[ignore = "live source check"]
fn live_urban_file_is_parseable() {
    let bytes = Client::new()
        .expect("HTTP client")
        .urban_csv("county_overall.csv")
        .expect("Urban CSV");
    assert!(
        statsapi_household_credit::urban_value(&bytes, "county", "34023", "totcoll")
            .expect("CSV parses")
            .is_some()
    );
}

#[test]
#[ignore = "live source check"]
fn live_ny_fed_workbook_is_parseable() {
    let bytes = Client::new()
        .expect("HTTP client")
        .ny_workbook()
        .expect("NY Fed workbook");
    assert!(statsapi_household_credit::ny_state_value(
        &bytes,
        "NJ",
        statsapi_household_credit::NyMetric::StateDebtPerCapita
    )
    .expect("workbook parses")
    .is_some());
}

#[test]
#[ignore = "live source check"]
fn live_federal_reserve_archive_is_parseable() {
    let bytes = Client::new()
        .expect("HTTP client")
        .county_archive()
        .expect("county archive");
    assert!(
        county_debt_to_income_bound(&bytes, "34023", CountyBound::Low)
            .expect("archive parses")
            .is_some()
    );
}
