use statsapi_hmda::decode_csv;

const FIXTURE: &[u8] = include_bytes!("fixtures/applications.csv");

#[test]
fn decodes_synthetic_hmda_csv_rows() {
    let rows = decode_csv(FIXTURE).expect("synthesized fixture follows HMDA CSV layout");
    assert_eq!(rows.len(), 6);
    assert_eq!(rows[0].census_tract, "01001020100");
    assert_eq!(rows[0].action_taken, "1");
    assert_eq!(rows[0].loan_amount, Some(210.0));
    assert_eq!(rows[1].derived_race, "Asian");
    assert_eq!(rows[2].loan_amount, None);
}

#[test]
fn rejects_csv_missing_required_columns() {
    let error = decode_csv(b"action_taken\n1\n").expect_err("required columns are validated");
    assert!(error.to_string().contains("census_tract"));
}

#[test]
#[ignore = "live request to the public FFIEC Data Browser API"]
fn live_county_csv_is_available() {
    let client = statsapi_hmda::Client::builder()
        .build()
        .expect("build HTTP client");
    let rows = client
        .applications(&statsapi_hmda::CsvRequest::new(2025, "counties", "01001"))
        .expect("fetch public county CSV");
    assert_ne!(rows.len(), 0);
}
