use statsapi_opportunity_insights::{Client, Dataset};

#[test]
#[ignore = "live Opportunity Atlas download"]
fn live_atlas_csv_downloads_and_decodes() {
    let table = Client::new()
        .expect("HTTP client builds")
        .fetch(Dataset::AtlasTracts)
        .expect("Atlas CSV is available");
    assert!(table.headers().iter().any(|header| header == "tract"));
    assert_ne!(table.rows().len(), 0);
}
