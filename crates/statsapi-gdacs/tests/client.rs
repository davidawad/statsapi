use serde_json::Value;
use statsapi_gdacs::{decode_alerts, Client, Level};

fn day(text: &str) -> chrono::NaiveDate {
    text.parse().unwrap_or_default()
}

#[test]
fn decodes_synthetic_collection_and_skips_malformed_features() {
    let value: Value =
        serde_json::from_str(include_str!("fixtures/alerts.json")).expect("fixture is valid JSON");
    let alerts = decode_alerts(&value).expect("fixture has features");
    assert_eq!(alerts.len(), 2);
    assert_eq!(alerts[0].level, Level::Orange);
    assert!(alerts[0].current);
    assert_eq!(alerts[0].iso3, ["EXA", "SMP"]);
    assert_eq!(alerts[1].level, Level::Red);
    assert_eq!(alerts[1].to, day("2026-06-09"));
}

#[test]
fn rejects_a_body_without_features() {
    assert!(decode_alerts(&serde_json::json!({"error": "x"})).is_err());
}

#[test]
fn levels_order_by_severity() {
    assert!(Level::Red > Level::Orange);
    assert_eq!(Level::parse("RED"), Some(Level::Red));
}

#[test]
fn url_requests_orange_and_red_for_the_range() {
    let url = Client::alerts_url(day("2026-01-01"), day("2026-11-23"));
    assert!(url.contains("alertlevel=Orange;Red"));
    assert!(url.contains("fromDate=2026-01-01&toDate=2026-11-23"));
}

#[test]
#[ignore = "queries the live GDACS API"]
fn live_alerts_query() {
    let (alerts, origin) = Client::new()
        .alerts(day("2026-10-01"), day("2026-11-23"))
        .expect("GDACS should answer");
    assert_eq!(origin.as_str(), "api");
    assert!(alerts.len() > 1, "{} alerts", alerts.len());
}
