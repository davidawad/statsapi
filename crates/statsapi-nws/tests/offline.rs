use serde_json::Value;
use statsapi_nws::{parse_alerts, parse_forecast, Client, Severity};

fn fixture(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or(Value::Null)
}

#[test]
fn parses_alert_layout_and_severity_order() {
    let alerts = parse_alerts(&fixture(include_str!("fixtures/alerts.json")))
        .expect("alerts fixture has features");
    assert_eq!(alerts.len(), 4, "every feature with an event is decoded");
    assert_eq!(alerts[0].severity, Severity::Severe);
    assert_eq!(alerts[3].severity, Severity::Unknown);
    assert!(Severity::Extreme > Severity::Severe, "severity is ordered");
    assert_eq!(Severity::Moderate.code(), 2);
    assert_eq!(alerts[1].headline, None);
}

#[test]
fn rejects_documents_without_features() {
    assert!(parse_alerts(&Value::Null).is_err(), "null has no features");
    assert!(parse_forecast(&Value::Null).is_err(), "null has no periods");
}

#[test]
fn forecast_picks_high_and_low_by_local_date() {
    let forecast = parse_forecast(&fixture(include_str!("fixtures/forecast.json")))
        .expect("forecast fixture has periods");
    assert_eq!(forecast.high_on("2030-01-01"), Some(60.0));
    assert_eq!(forecast.low_on("2030-01-01"), Some(41.0));
    assert_eq!(forecast.high_on("2030-01-02"), Some(68.0), "20 C is 68 F");
    assert_eq!(forecast.low_on("2030-01-02"), Some(32.0), "0 C is 32 F");
    assert_eq!(forecast.high_on("2030-01-09"), None);
}

#[test]
fn builds_point_urls() {
    let client = Client::with_base("test", "https://example.test/");
    assert_eq!(
        client.alerts_url(33.58, -101.85),
        "https://example.test/alerts/active?point=33.5800,-101.8500"
    );
}

#[test]
#[ignore = "queries the live NWS API"]
fn live_alerts_and_forecast() {
    let client = Client::new("statsapi-nws test client");
    client
        .alerts_active(40.56, -74.32)
        .expect("NWS alerts answer");
    let (forecast, _) = client
        .forecast(40.56, -74.32)
        .expect("NWS forecast answers");
    assert_ne!(forecast.periods.len(), 0, "forecast has periods");
}
