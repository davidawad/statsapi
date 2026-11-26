#![expect(
    clippy::expect_used,
    reason = "tests need contextual panic messages for setup failures"
)]

use chrono::NaiveDate;
use statsapi_cisa_kev::{Catalog, Client, Error};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

const FIXTURE: &[u8] = include_bytes!("fixtures/catalog.json");

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 23).expect("valid date")
}

/// Serves each canned response to one connection, in order, then stops.
fn serve(responses: Vec<(u16, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let address = listener.local_addr().expect("local address");
    std::thread::spawn(move || {
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).expect("write head");
            stream.write_all(&body).expect("write body");
        }
    });
    format!("http://{address}/feed")
}

fn client() -> Client {
    Client::new()
        .expect("client builds")
        .with_retry_pause(Duration::from_millis(1))
}

#[test]
fn summarizes_windows_ransomware_and_vendors() {
    let catalog = Catalog::from_reader(FIXTURE).expect("fixture decodes");
    let summary = catalog.summarize(today()).expect("summary");
    assert_eq!(summary.catalog_size, 8);
    assert_eq!(summary.added_7d, 3, "2026-11-17 onward, today included");
    assert_eq!(summary.added_30d, 4);
    assert_eq!(summary.added_90d, 6);
    assert_eq!(summary.ransomware_share, 37.5);
    assert_eq!(summary.ransomware_share_90d, 33.33);
    let top: Vec<_> = summary
        .top_vendors_90d
        .iter()
        .map(|v| (v.vendor.as_str(), v.added))
        .collect();
    assert_eq!(top, [("Acme", 3), ("Borealis", 2), ("Cinder", 1)]);
}

#[test]
fn rejects_bad_dates_and_bad_json() {
    let bad = br#"{"vulnerabilities":[{"cveID":"C","vendorProject":"V","dateAdded":"soon"}]}"#;
    let catalog = Catalog::from_reader(&bad[..]).expect("decodes");
    assert!(matches!(catalog.summarize(today()), Err(Error::Date(_))));
    assert!(matches!(
        Catalog::from_reader(&b"<html>"[..]),
        Err(Error::Decode(_))
    ));
}

#[test]
fn falls_back_to_the_next_catalog_url_when_the_first_refuses() {
    let refused = serve(vec![(403, b"denied".to_vec())]);
    let good = serve(vec![(200, FIXTURE.to_vec())]);
    let catalog = client()
        .with_kev_urls(vec![refused, good.clone()])
        .fetch_catalog()
        .expect("mirror answers");
    assert_eq!(catalog.source_url, good);
    assert_eq!(catalog.vulnerabilities.len(), 8);
}

#[test]
fn reports_the_status_when_every_url_fails() {
    let refused = serve(vec![(403, Vec::new())]);
    let error = client()
        .with_kev_urls(vec![refused])
        .fetch_catalog()
        .expect_err("refused");
    assert!(
        matches!(error, Error::Status { status: 403, .. }),
        "{error}"
    );
}

#[test]
fn nvd_count_retries_429_and_reads_total_results() {
    let url = serve(vec![
        (429, Vec::new()),
        (200, br#"{"resultsPerPage":1,"totalResults":42}"#.to_vec()),
    ]);
    let now = today().and_hms_opt(12, 0, 0).expect("time");
    let (count, requested) = client()
        .with_nvd_url(url)
        .critical_cves_published(now, 7)
        .expect("count");
    assert_eq!(count, 42);
    assert!(requested.contains("cvssV3Severity=CRITICAL"));
    assert!(requested.contains("pubStartDate=2026-11-16T12:00:00.000"));
}

#[test]
fn nvd_window_is_bounded() {
    let now = today().and_hms_opt(0, 0, 0).expect("time");
    assert!(client().critical_cves_published(now, 121).is_err());
}
