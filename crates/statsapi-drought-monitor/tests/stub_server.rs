use statsapi_drought_monitor::{Area, Client, Date, Error};
use std::error::Error as StdError;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

type Result<T> = std::result::Result<T, Box<dyn StdError>>;

fn serve(
    status: &str,
    body: &'static str,
) -> Result<(String, JoinHandle<std::io::Result<String>>)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let base = format!("http://{}", listener.local_addr()?);
    let status = status.to_owned();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept()?;
        let mut buffer = [0_u8; 2048];
        let read = stream.read(&mut buffer)?;
        let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
        let reply = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(reply.as_bytes())?;
        Ok(request)
    });
    Ok((base, handle))
}

fn window() -> Result<(Date, Date)> {
    Ok((Date::parse("2026-11-01")?, Date::parse("2026-11-23")?))
}

#[test]
fn weekly_fetches_and_parses_a_live_shaped_response() {
    let (base, handle) = serve(
        "200 OK",
        r#"[{"mapDate":"2026-11-17T00:00:00","fips":"01001","none":0.0,"d0":100.0,"d1":24.16,"d2":0.0,"d3":0.0,"d4":0.0}]"#,
    )
    .expect("server starts");
    let (start, end) = window().expect("window");
    let weeks = Client::with_base_url(&base)
        .expect("client")
        .weekly(Area::County, "01001", start, end)
        .expect("weeks");
    assert_eq!(weeks.len(), 1);
    assert!((weeks[0].d1 - 24.16).abs() < 1e-9);
    let request = handle.join().expect("thread").expect("io");
    assert!(request
        .starts_with("GET /CountyStatistics/GetDroughtSeverityStatisticsByAreaPercent?aoi=01001"));
}

#[test]
fn empty_result_is_no_data() {
    let (base, _handle) = serve("200 OK", "[]").expect("server starts");
    let (start, end) = window().expect("window");
    let result =
        Client::with_base_url(&base)
            .expect("client")
            .weekly(Area::State, "01", start, end);
    assert!(matches!(result, Err(Error::NoData { .. })));
}

#[test]
fn client_errors_are_not_retried() {
    let (base, handle) = serve("400 Bad Request", "bad").expect("server starts");
    let (start, end) = window().expect("window");
    let result =
        Client::with_base_url(&base)
            .expect("client")
            .weekly(Area::State, "zz", start, end);
    assert!(matches!(result, Err(Error::Status { status: 400, .. })));
    handle
        .join()
        .expect("thread")
        .expect("single request served");
}
