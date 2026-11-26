use statsapi_worldbank::Client;

#[test]
#[ignore = "live World Bank API smoke test; run once in the source pod"]
fn fetches_a_live_indicator_response() {
    let client = Client::new(None).expect("build World Bank client");
    let response = client
        .indicator("USA", "SP.POP.TOTL", None)
        .expect("fetch World Bank indicator");
    let observation =
        Client::latest_observation(&response).expect("API has a non-null observation");
    assert!(observation
        .get("date")
        .and_then(serde_json::Value::as_str)
        .is_some());
    assert!(observation
        .get("value")
        .is_some_and(serde_json::Value::is_number));
}
