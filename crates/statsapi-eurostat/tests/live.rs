use statsapi_eurostat::{Client, NutsLevel};

#[test]
#[ignore = "live Eurostat and GISCO smoke test; run once in the source pod"]
fn live_region_lookup_and_observation() {
    let client = Client::new().expect("build Eurostat client");
    let region = client
        .nuts_for_point(48.1371, 11.5754, NutsLevel::Two, 2024)
        .expect("fetch GISCO boundaries")
        .expect("Munich is in a NUTS 2 region");
    assert_eq!(region.id, "DE21");
    let observation = client
        .latest(
            "nama_10r_2gdp",
            &[("geo", region.id.as_str()), ("unit", "PPS_EU27_2020_HAB")],
        )
        .expect("fetch Eurostat observation")
        .expect("Oberbayern has a GDP value");
    assert!(observation.value > 10_000.0, "got {observation:?}");
}
