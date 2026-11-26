use chrono::Utc;
use statsapi_cisa_kev::Client;

#[test]
#[ignore = "live network"]
fn live_catalog_and_nvd_count() {
    let client = Client::new().expect("client");
    let now = Utc::now().naive_utc();
    let catalog = client.fetch_catalog().expect("catalog");
    let summary = catalog.summarize(now.date()).expect("summary");
    assert!(summary.catalog_size > 1000, "{summary:?}");
    assert!(summary.added_90d >= summary.added_30d);
    eprintln!("{} {summary:?}", catalog.source_url);
    let (critical, url) = client.critical_cves_published(now, 7).expect("nvd");
    eprintln!("nvd critical 7d = {critical} {url}");
}
