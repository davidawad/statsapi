use serde_json::{json, Value};
use statsapi_reliefweb::{decode_disasters, Client, DisasterFilter, Status};

fn day(text: &str) -> chrono::NaiveDate {
    text.parse().unwrap_or_default()
}

#[test]
fn decodes_synthetic_list_and_skips_malformed_rows() {
    let value: Value =
        serde_json::from_str(include_str!("fixtures/disasters.json")).expect("fixture is JSON");
    let found = decode_disasters(&value).expect("fixture has data");
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].id, 900_002);
    assert_eq!(found[0].status, Status::Ongoing);
    assert_eq!(found[0].date, day("2026-10-24"));
    assert_eq!(found[0].primary_type, "Flood");
    assert_eq!(found[1].url, "");
}

#[test]
fn rejects_a_body_without_data() {
    assert!(decode_disasters(&json!({"error": "x"})).is_err());
}

#[test]
fn disaster_body_filters_country_status_and_date() {
    let filter = DisasterFilter {
        statuses: vec![Status::Ongoing, Status::Alert],
        since: Some(day("2026-01-01")),
        limit: Some(5),
    };
    let body = Client::disasters_body("PHL", &filter);
    let conditions = &body["filter"]["conditions"];
    assert_eq!(conditions[0]["value"], "phl");
    assert_eq!(conditions[1]["value"], json!(["ongoing", "alert"]));
    assert_eq!(conditions[2]["value"]["from"], "2026-01-01T00:00:00+00:00");
    assert_eq!(body["limit"], 5);
}

#[test]
fn report_body_counts_without_rows() {
    let body = Client::reports_body("PHL", day("2026-10-26"));
    assert_eq!(body["limit"], 0);
    assert_eq!(body["filter"]["conditions"][1]["field"], "date.created");
}

#[test]
#[ignore = "queries the live ReliefWeb API"]
fn live_disasters_and_reports() {
    let client = Client::new();
    let (found, origin) = client
        .disasters("PHL", &DisasterFilter::default())
        .expect("ReliefWeb should answer");
    assert_eq!(origin.as_str(), "api");
    assert_ne!(found.len(), 0);
    let (reports, _) = client
        .report_count("PHL", day("2020-01-01"))
        .expect("ReliefWeb should answer");
    assert!(reports > 0);
}
