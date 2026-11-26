use statsapi_noaa::{parse_details, Client};

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "fixture assertions verify the normalized CSV layout"
)]
fn parses_synthetic_storm_detail_layout() -> Result<(), Box<dyn std::error::Error>> {
    let rows = parse_details(include_bytes!("fixtures/details.csv"))?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].county, "MIDDLESEX");
    assert_eq!(rows[0].county_fips, "023");
    assert_eq!(rows[0].property_damage, 1200.0);
    let _client = Client::new();
    Ok(())
}
#[test]
#[ignore = "downloads current NOAA Storm Events bulk files"]
fn live_storm_events_index_has_detail_files() {
    let rows = Client::new().details(2024).expect("NOAA has 2024 events");
    assert_ne!(rows.len(), 0);
}
