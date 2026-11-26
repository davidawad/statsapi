use statsapi_nara::Client;

#[test]
fn builds_bounded_search_and_decodes_fixture() {
    let url = Client::search_url("cia records", 25).expect("search URL builds");
    assert!(url.contains("q=cia+records"));
    assert!(url.contains("limit=25"));
    let response = statsapi_nara::decode_response(br#"{"opaResponse":{"results":[]}}"#)
        .expect("fixture decodes");
    assert!(response["opaResponse"]["results"].is_array());
}
