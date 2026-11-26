use statsapi_ioda::{Client, EntityType, Error};
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

#[test]
fn episodes_fetches_and_clips_to_the_window() {
    let (base, handle) = serve(
        "200 OK",
        r#"{"data":[{"entity":{"name":"Iraq"},"from":100,"until":200,"datasource":"bgp"},{"entity":{"name":"Iraq"},"from":5000,"until":6000,"datasource":"gtr"}]}"#,
    )
    .expect("server starts");
    let episodes = Client::with_base_url(&base)
        .expect("client")
        .episodes(EntityType::Country, "IQ", 1000, 9000)
        .expect("episodes");
    assert_eq!(episodes.len(), 1);
    assert_eq!(episodes[0].from, 5000);
    let request = handle.join().expect("thread").expect("io");
    assert!(request
        .starts_with("GET /outages/events?entityType=country&entityCode=IQ&from=1000&until=9000"));
}

#[test]
fn region_code_matches_name_case_insensitively() {
    let (base, _handle) = serve(
        "200 OK",
        r#"{"data":[{"code":"4416","name":"California"},{"code":"4417","name":"Texas"}]}"#,
    )
    .expect("server starts");
    let client = Client::with_base_url(&base).expect("client");
    assert_eq!(
        client.region_code("US", "california").expect("lookup"),
        Some("4416".to_owned())
    );
}

#[test]
fn client_errors_are_not_retried() {
    let (base, handle) = serve("400 Bad Request", "bad").expect("server starts");
    let result =
        Client::with_base_url(&base)
            .expect("client")
            .episodes(EntityType::Country, "ZZ", 0, 1);
    assert!(matches!(result, Err(Error::Status { status: 400, .. })));
    handle
        .join()
        .expect("thread")
        .expect("single request served");
}
