#![expect(clippy::expect_used, reason = "test helpers assert by panicking")]

use statsapi_launch_library::{Client, Error, Filter, Listing, Origin};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const COUNT: &str = include_str!("fixtures/count.json");
const AGENCIES_P1: &str = include_str!("fixtures/agencies1.json");
const AGENCIES_P2: &str = include_str!("fixtures/agencies2.json");
const EMPTY: &str = include_str!("fixtures/empty.json");

/// Serves one canned `(status, body)` per connection, recording request lines.
fn serve(replies: Vec<(u16, &'static str)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    let root = base.clone();
    thread::spawn(move || {
        for (status, body) in replies {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0_u8; 4096];
            let read = stream.read(&mut buffer).expect("read");
            let request = String::from_utf8_lossy(&buffer[..read]);
            let line = request.lines().next().unwrap_or_default().to_owned();
            log.lock().expect("log").push(line);
            let body = body.replace("BASE", &root);
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).expect("write");
        }
    });
    (base, seen)
}

fn client(base: &str) -> Client {
    Client::builder("test/1.0")
        .base_url(base)
        .retries(1, Duration::from_millis(1))
        .build()
        .expect("client")
}

#[test]
fn count_sends_filters_and_reads_count() {
    let (base, seen) = serve(vec![(200, COUNT)]);
    let filter = Filter {
        provider_ids: vec![121, 5],
        net_from: Some("2026-01-01".into()),
        status_ids: vec![4, 7],
    };
    let got = client(&base)
        .count(Listing::Previous, &filter)
        .expect("count");
    assert_eq!(got.count, 42);
    assert_eq!(got.origin, Origin::Api);
    let line = seen.lock().expect("log")[0].clone();
    assert!(
        line.contains("/launch/previous/?limit=1&mode=list"),
        "{line}"
    );
    assert!(line.contains("lsp__ids=121,5"), "{line}");
    assert!(line.contains("net__gte=2026-01-01"), "{line}");
    assert!(line.contains("status__ids=4,7"), "{line}");
}

#[test]
fn next_launch_reads_first_result_or_none() {
    let (base, _) = serve(vec![(200, COUNT), (200, EMPTY)]);
    let api = client(&base);
    let next = api.next_launch(&[]).expect("next").expect("one launch");
    assert_eq!(next.name, "Synthetic Rocket | Test Payload");
    assert_eq!(next.net, "2030-01-02T03:04:05Z");
    assert!(api.next_launch(&[7]).expect("next").is_none());
}

#[test]
fn throttled_request_falls_back_to_fresh_cache_only() {
    let dir = std::env::temp_dir().join(format!("sll-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (base, _) = serve(vec![(200, COUNT), (429, "{}"), (429, "{}")]);
    let build = |ttl: Duration| {
        Client::builder("test/1.0")
            .base_url(&base)
            .fallback_cache(&dir, ttl)
            .retries(1, Duration::from_millis(1))
            .build()
            .expect("client")
    };
    let filter = Filter::default();
    let live = build(Duration::from_secs(60))
        .count(Listing::Previous, &filter)
        .expect("live");
    assert_eq!(live.origin, Origin::Api);
    let cached = build(Duration::from_secs(60))
        .count(Listing::Previous, &filter)
        .expect("cached");
    assert_eq!((cached.count, cached.origin), (42, Origin::Cache));
    thread::sleep(Duration::from_millis(20));
    let stale = build(Duration::from_millis(1)).count(Listing::Previous, &filter);
    assert!(matches!(stale, Err(Error::HttpStatus(429))), "{stale:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn malformed_body_is_an_error() {
    let (base, _) = serve(vec![(200, "{\"results\": []}")]);
    let got = client(&base).count(Listing::Previous, &Filter::default());
    assert!(matches!(got, Err(Error::InvalidResponse(_))), "{got:?}");
}

#[test]
#[ignore = "calls the live Launch Library API (free tier: ~15 requests/hour)"]
fn live_count_and_next_launch() {
    let api = Client::builder("statsapi-launch-library-test/0.1 (contact: ops@example.org)")
        .build()
        .expect("client");
    let providers = api.providers("FRA").expect("live providers");
    assert!(!providers.ids.is_empty(), "{providers:?}");
    let filter = Filter {
        provider_ids: providers.ids.clone(),
        net_from: Some("2020-01-01".into()),
        ..Filter::default()
    };
    let got = api.count(Listing::Previous, &filter).expect("live count");
    assert!(got.count > 0, "{got:?}");
    let next = api.next_launch(&providers.ids).expect("live next");
    eprintln!("LIVE {got:?} next={next:?}");
}

#[test]
fn providers_keep_exact_country_and_follow_pages() {
    let (base, seen) = serve(vec![(200, AGENCIES_P1), (200, AGENCIES_P2)]);
    let got = client(&base).providers("FRA").expect("providers");
    assert_eq!(got.ids, vec![3, 7, 9]);
    assert_eq!(seen.lock().expect("log").len(), 2);
    assert!(got.source_url.contains("country_code=FRA"));
}

#[test]
fn next_launch_asks_only_for_unflown_statuses() {
    let (base, seen) = serve(vec![(200, EMPTY)]);
    assert!(client(&base).next_launch(&[]).expect("next").is_none());
    let line = seen.lock().expect("log")[0].clone();
    assert!(line.contains("/launch/upcoming/"), "{line}");
    assert!(line.contains("status__ids=1,2,5,8"), "{line}");
}
