use serde_json::Value;
use statsapi_envirofacts::{npl_url, parse_count, parse_npl, summarize_tri, Client, TriArea};

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "fixture assertions compare the synthesized upstream shape"
)]
fn summarizes_tri_rows_from_a_synthesized_fixture() -> Result<(), serde_json::Error> {
    let rows: Value = serde_json::from_str(include_str!("fixtures/tri_rows.json"))?;
    let summary = summarize_tri(rows.as_array().map_or(&[][..], Vec::as_slice));
    assert_eq!(summary.facilities, 2);
    assert!((summary.onsite_release_lbs - 1125.5).abs() < 1e-9);
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "fixture assertions compare the synthesized upstream shape"
)]
fn parses_npl_features_nearest_first() -> Result<(), Box<dyn std::error::Error>> {
    let value: Value = serde_json::from_str(include_str!("fixtures/npl.json"))?;
    let sites = parse_npl(&value, 40.5434, -74.3626)?;
    assert_eq!(sites[0].name, "NEAR SITE");
    assert!(sites[0].distance_km < 3.0);
    assert!(sites[1].distance_km > 15.0);
    Ok(())
}

#[test]
fn builds_urls_and_reads_counts() {
    let url = Client::tri_url(&TriArea::Zip("08840".into()), 2024, 0, 1999);
    assert!(url.ends_with("/tri_facility/zip_code/08840/tri_reporting_form/reporting_year/2024/tri_release_qty/rows/0:1999/JSON"));
    assert!(npl_url(40.5, -74.3, 5.0).contains("distance=5000"));
    assert_eq!(
        parse_count(&serde_json::json!([{"TOTALQUERYRESULTS": 7}])),
        7
    );
    assert_eq!(parse_count(&serde_json::json!([])), 0);
}

#[test]
#[ignore = "queries live EPA Envirofacts and ArcGIS hosts"]
#[expect(
    clippy::panic_in_result_fn,
    reason = "fixture assertions compare the synthesized upstream shape"
)]
fn live_tri_and_npl_queries() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let year = client.latest_tri_year(2026)?;
    let summary = client.tri_summary(&TriArea::Zip("08840".into()), year)?;
    assert!(summary.facilities > 0);
    let sites = client.npl_sites_near(40.5434, -74.3626, 25.0)?;
    assert!(!sites.is_empty(), "no NPL site within 25 km");
    eprintln!(
        "LIVE_EVIDENCE tri_year={year} {summary:?} nearest={:?}",
        sites[0]
    );
    Ok(())
}
