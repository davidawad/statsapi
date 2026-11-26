use statsapi_gleif::Client;

#[test]
#[ignore = "live GLEIF API smoke test; run once in the source pod"]
fn live_us_entity_count() {
    let count = Client::new()
        .expect("client")
        .count(&[("entity.legalAddress.country", "US")])
        .expect("live count");
    assert!(count.total > 100_000);
    assert!(count.publish_date.is_some());
}
