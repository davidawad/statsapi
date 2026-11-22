use statsapi_county_health_rankings::{Client, ANNUAL_CSV_URL};

#[test]
#[ignore = "live CHR&R endpoint check"]
fn fetches_and_decodes_live_annual_file() {
    let csv = Client::default()
        .fetch_annual_csv()
        .expect("fetch public annual CSV");
    assert!(Client::default()
        .measure(&csv, "34023", "Premature Death")
        .expect("decode county row")
        .is_some());
    println!("live CSV endpoint checked: {ANNUAL_CSV_URL}");
}
