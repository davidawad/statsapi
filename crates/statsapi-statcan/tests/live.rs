use statsapi_statcan::{Client, CANADA_CODE};

#[test]
#[ignore = "live Statistics Canada WDS smoke test; run once in the source pod"]
fn live_canada_unemployment_rate() {
    let client = Client::new().expect("client");
    let (geo, _, observation) = client
        .latest_for_geography(14_100_287, "{geo}.7.1.1.1.1", CANADA_CODE)
        .expect("live WDS value");
    assert_eq!(geo.name, "Canada");
    assert!((0.0..100.0).contains(&observation.value));
}
