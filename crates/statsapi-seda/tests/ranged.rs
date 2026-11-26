//! Keyed range reads against a local server that honors `Range`.

#![expect(
    clippy::expect_used,
    reason = "test helpers panic when local server setup fails"
)]

use statsapi_seda::{Client, Dataset, Level};
use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// A sorted district CSV of 3000 ids, three rows each, with long filler columns.
fn release() -> Vec<u8> {
    let mut csv = String::from("sedaadmin,sedaadminname,filler,cs_mn_avg_mth_ol\n");
    for id in 1..=3000_u64 {
        for year in 0..3 {
            writeln!(csv, "{id},\"District, {id}\",{},{year}.5", "x".repeat(300))
                .expect("write to a String");
        }
    }
    csv.into_bytes()
}

/// Serve `body` with range support (or ignore `Range` when `ranges` is false);
/// returns the base URL and the number of body bytes sent.
fn serve(body: Vec<u8>, ranges: bool) -> (String, Arc<AtomicU64>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let base = format!("http://{}/", listener.local_addr().expect("addr"));
    let sent = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&sent);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let body = body.clone();
            let counter = Arc::clone(&counter);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut range = None;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        let (from, to) = value.trim().split_once('-').expect("range");
                        range = Some((
                            from.parse::<usize>().expect("from"),
                            to.parse::<usize>().unwrap_or(body.len() - 1),
                        ));
                    }
                }
                let mut out = stream;
                let (status, slice, extra) = match range.filter(|_| ranges) {
                    Some((from, to)) => {
                        let to = to.min(body.len() - 1);
                        (
                            "206 Partial Content",
                            &body[from..=to],
                            format!("Content-Range: bytes {from}-{to}/{}\r\n", body.len()),
                        )
                    }
                    None => ("200 OK", &body[..], String::new()),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    slice.len()
                );
                if out.write_all(head.as_bytes()).is_ok() && out.write_all(slice).is_ok() {
                    counter.fetch_add(slice.len() as u64, Ordering::SeqCst);
                }
                let _ = out.flush();
                let _ = reader.read(&mut [0_u8; 1]);
            });
        }
    });
    (base, sent)
}

fn fetch(base: &str, code: &str) -> statsapi_seda::Table {
    Client::new()
        .expect("client")
        .with_release_base(base)
        .fetch(Level::District, Dataset::Achievement, code)
        .expect("fetch")
}

#[test]
fn bisects_to_one_geography_without_reading_the_file() {
    let body = release();
    let total = body.len() as u64;
    let (base, sent) = serve(body, true);
    for code in ["1", "1777", "3000"] {
        let table = fetch(&base, code);
        assert_eq!(table.rows.len(), 3, "rows for {code}");
        assert!(table.rows.iter().all(|row| row.get(0) == Some(code)));
    }
    assert_eq!(fetch(&base, "99999").rows.len(), 0, "absent id has no rows");
    assert!(
        sent.load(Ordering::SeqCst) < total,
        "four lookups read less than one whole file"
    );
}

#[test]
fn streams_the_whole_file_when_the_server_ignores_ranges() {
    let (base, _) = serve(release(), false);
    let table = fetch(&base, "1777");
    assert_eq!(table.rows.len(), 3);
}
