use serde_json::Value;
use statsapi_gdelt::{decode_timeline, Client, Error};

#[test]
fn decodes_synthetic_doc_layout_and_sorts_dates() {
    let value: Value = serde_json::from_str(include_str!("fixtures/timelinevolraw.json"))
        .expect("synthetic DOC fixture is valid JSON");
    let points = decode_timeline(&value).expect("fixture has timeline data");
    assert_eq!(
        points
            .iter()
            .map(|point| point.date.as_str())
            .collect::<Vec<_>>(),
        ["20250101", "20250102", "20250103"]
    );
    assert_eq!(points[2].value, 17.0);
}

#[test]
fn constructs_query_with_expected_doc_parameters() {
    let url = Client::timeline_url("\"sample place\"", "timelinevolraw", "20250101", "20250130")
        .expect("fixed API endpoint is valid");
    assert!(url.contains("query=%22sample+place%22"));
    assert!(url.contains("startdatetime=20250101000000"));
}

#[test]
fn offline_client_returns_cache_miss_without_network_access() {
    let client = Client::new()
        .expect("HTTP client initializes")
        .offline_cache_only();
    assert!(matches!(
        client.fetch_timeline("https://api.gdeltproject.org/api/v2/doc/doc"),
        Err(Error::OfflineCacheMiss)
    ));
}

#[test]
#[ignore = "live GDELT DOC request"]
fn live_timeline_request() {
    let client = Client::new().expect("HTTP client initializes");
    assert!(client
        .timeline("\"OpenAI\"", "timelinevolraw", "20261101", "20261122")
        .is_ok());
}
