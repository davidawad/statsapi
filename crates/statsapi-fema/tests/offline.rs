use statsapi_fema::Client;

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "fixture assertions compare the preserved upstream JSON shape"
)]
fn parses_synthesized_openfema_collection_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let value: serde_json::Value = serde_json::from_str(include_str!("fixtures/collection.json"))?;
    assert_eq!(value["NfipClaims"][0]["reportedZipCode"], "08840");
    assert_eq!(value["metadata"]["count"], 1);
    let _client = Client::new().offline(true);
    Ok(())
}

#[test]
#[ignore = "queries the live FEMA OpenFEMA host"]
fn live_openfema_claim_query() {
    let url="https://www.fema.gov/api/open/v3/NfipClaims?$filter=reportedZipCode%20eq%20%2708840%27&$top=1";
    let response = Client::new()
        .get_json(url)
        .expect("OpenFEMA should return a JSON collection");
    assert!(response["NfipClaims"].is_array());
}

#[test]
#[cfg(feature = "cache")]
fn live_answer_is_stored_and_served_only_when_the_source_goes_away() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}/data", listener.local_addr().expect("addr"));
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        let body = r#"{"rows":[1]}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("respond");
    });
    let dir = std::env::temp_dir().join(format!("fema-ephemeral-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let client = Client::new().with_cache_dir(&dir);
    client.get_json(&url).expect("live answer");
    assert_eq!(client.trail().take(), Some(ephemeral_cache::Source::Api));
    server.join().expect("server exits and stops listening");
    let cached = client
        .get_json(&url)
        .expect("cache answers when live fails");
    assert_eq!(cached["rows"][0], 1);
    assert_eq!(client.trail().take(), Some(ephemeral_cache::Source::Cache));
    let _ = std::fs::remove_dir_all(dir);
}
