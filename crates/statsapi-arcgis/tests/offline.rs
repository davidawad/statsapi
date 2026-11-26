use statsapi_arcgis::Client;
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "fixture assertions compare the preserved upstream JSON shape"
)]
fn fixture_preserves_arcgis_feature_shape() -> Result<(), Box<dyn std::error::Error>> {
    let value: serde_json::Value = serde_json::from_str(include_str!("fixtures/features.json"))?;
    assert_eq!(value["features"][0]["attributes"]["COUNTY"], "34023");
    let _client = Client::new().offline(true);
    Ok(())
}
#[test]
#[ignore = "queries public ArcGIS feature services"]
fn live_fema_nri_query() {
    let url="https://services5.arcgis.com/W1uyphp8h2tna3qJ/ArcGIS/rest/services/NRI_GDB_Counties_%282%29/FeatureServer/0/query?where=STCOFIPS%3D%2734023%27&outFields=*&returnGeometry=false&f=json";
    let data = Client::new()
        .get_json(url)
        .expect("FEMA NRI service responds");
    assert!(data["features"].is_array());
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
    let dir = std::env::temp_dir().join(format!("arcgis-ephemeral-{}", std::process::id()));
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
