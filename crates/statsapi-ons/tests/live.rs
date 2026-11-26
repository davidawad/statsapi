use statsapi_ons::{latest, Client};

#[test]
#[ignore = "live ONS API smoke test; run once in the source pod"]
fn fetches_live_population_for_westminster() {
    let client = Client::new().expect("build ONS client");
    let version = client
        .version("mid-year-pop-est", None)
        .expect("resolve dataset version");
    let order = client.options(&version, "time").expect("time options");
    let rows = client
        .observations(
            &version,
            &[
                ("time", "*"),
                ("geography", "E09000033"),
                ("age", "total"),
                ("sex", "all"),
            ],
        )
        .expect("fetch observations");
    let newest = latest(&rows, &order).expect("a published year");
    assert!(newest.value > 10_000.0);
}

#[test]
#[ignore = "live postcodes.io smoke test; run once in the source pod"]
fn reverse_geocodes_westminster() {
    let client = Client::new().expect("build ONS client");
    let found = client
        .local_authority(51.5072, -0.1276)
        .expect("reverse geocode")
        .expect("a local authority");
    assert_eq!(found.code, "E09000033");
}
