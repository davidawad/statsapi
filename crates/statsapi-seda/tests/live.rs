use statsapi_seda::{Client, Dataset, Level};

#[test]
#[ignore = "downloads a Stanford SEDA file"]
fn live_county_csv_has_matching_geography() {
    let table = Client::new()
        .expect("HTTP client builds")
        .fetch(Level::County, Dataset::Achievement, "00001")
        .expect("SEDA source responds");
    assert!(table.source_url.contains("county_poolsub_cs_6.0.csv"));
}
