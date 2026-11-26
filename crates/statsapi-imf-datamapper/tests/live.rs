use statsapi_imf_datamapper::{horizon, Client};

#[test]
#[ignore = "live IMF DataMapper smoke test; run once in the source pod"]
fn fetches_a_live_country_series() {
    let client = Client::new().expect("build client");
    let series = client
        .country_series("NGDP_RPCH", "USA")
        .expect("fetch indicator")
        .expect("USA is published");
    let actual = horizon(&series, 2026, 0).expect("an actual year exists");
    assert!(actual.year < 2026);
    assert!(horizon(&series, 2026, 1).is_some());
}
