use statsapi_census_bds::{parse_newest, query_for, Client, Geography, Route};
use std::io::{Read, Write};
use std::net::TcpListener;

const FIXTURE: &str = include_str!("fixtures/state.json");

#[test]
fn newest_year_with_published_values_wins_and_suppressed_rows_are_skipped() {
    let o = parse_newest(FIXTURE).expect("parse");
    assert_eq!(o.year, 2022);
    assert_eq!(o.get("JOB_CREATION_RATE"), Some(15.2));
    assert_eq!(o.get("ESTAB"), Some(26000.0));
    assert_eq!(o.get("NOPE"), None);
}

#[test]
fn non_tables_are_errors() {
    assert!(parse_newest("{}").is_err());
    assert!(parse_newest("<html>").is_err());
    assert!(parse_newest("[[\"NAME\"]]").is_err());
}

#[test]
fn queries_name_the_geography_and_all_sector_series() {
    let q = query_for(&Geography::County("10".into(), "003".into()));
    assert!(q.contains("for=county:003&in=state:10"));
    assert!(q.contains("NAICS=00"));
    assert!(query_for(&Geography::Us).contains("for=us:1"));
}

/// One-shot local server: serves `responses` in order, one per connection.
#[expect(
    clippy::expect_used,
    reason = "test helper: failing to bind is a test failure"
)]
fn serve(responses: Vec<String>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        for response in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{addr}/bds")
}

fn ok(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[test]
fn live_success_is_api_and_failure_falls_back_to_the_fresh_cache() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fail = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    let base = serve(vec![ok(FIXTURE), fail.to_string()]);
    let client =
        Client::with_base(&base, Some("secret".into()), Some(dir.path().into())).expect("client");
    let geo = Geography::State("99".into());
    let first = client.newest(&geo).expect("live");
    assert_eq!(first.source, Route::Api);
    assert!(!first.url.contains("secret"));
    let second = client.newest(&geo).expect("cache fallback");
    assert_eq!(second.source, Route::Cache);
    assert_eq!(second.year, 2022);
}

#[test]
fn redirect_to_missing_key_page_is_credential_required() {
    let redirect =
        "HTTP/1.1 302 Found\r\nLocation: /missing_key.html\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    let base = serve(vec![redirect.to_string()]);
    let client = Client::with_base(&base, None, None).expect("client");
    let e = client.newest(&Geography::Us).expect_err("no key");
    assert!(e.to_string().contains("credential_required"));
}
