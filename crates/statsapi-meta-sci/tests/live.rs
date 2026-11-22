use statsapi_meta_sci::{Client, Source};

fn live_client() -> Client {
    Client::new().with_cache_dir(std::env::temp_dir().join("statsapi-meta-sci-live-cache"))
}

#[test]
#[ignore = "downloads the live HDX country CSV"]
fn live_country_file_decodes() {
    assert!(
        !live_client()
            .rows(Source::Country, "US")
            .expect("HDX country SCI download")
            .is_empty(),
        "country response has rows"
    );
}

#[test]
#[ignore = "downloads the live HDX county CSV"]
fn live_county_file_decodes() {
    assert!(
        !live_client()
            .rows(Source::County, "06037")
            .expect("HDX county SCI download")
            .is_empty(),
        "county response has rows"
    );
}

#[test]
#[ignore = "downloads the 1.8 GB live HDX ZCTA ZIP"]
fn live_zcta_archive_decodes() {
    assert!(
        !live_client()
            .rows(Source::Zcta, "90001")
            .expect("HDX ZCTA SCI download")
            .is_empty(),
        "ZCTA response has rows"
    );
}
