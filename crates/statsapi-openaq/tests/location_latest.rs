use statsapi_openaq::{Client, Location};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;

#[test]
fn country_latest_uses_each_location_endpoint_and_its_pm25_sensor() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local server");
    let address = listener.local_addr().expect("server address");
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("client connects");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let mut request = String::new();
        reader.read_line(&mut request).expect("request line");
        let mut headers = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("request header");
            if line == "\r\n" || line.is_empty() {
                break;
            }
            headers.push_str(&line);
        }
        let body = include_str!("fixtures/latest.json");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let mut stream = stream;
        stream
            .write_all(response.as_bytes())
            .expect("write response");
        (request, headers)
    });

    let client =
        Client::with_base("test-key", &format!("http://{address}/v3")).expect("client builds");
    let readings = client
        .pm25_latest(&[Location {
            id: 7,
            name: "Test station".into(),
            lat: 40.71,
            lon: -74.0,
            pm25_sensor: 101,
        }])
        .expect("latest reading loads");
    let (request, headers) = server.join().expect("server completes");

    assert!(request.starts_with("GET /v3/locations/7/latest?limit=100 HTTP/1.1"));
    assert!(headers.to_ascii_lowercase().contains("x-api-key: test-key"));
    assert_eq!(readings.len(), 1);
    assert_eq!(readings[0].sensor, 101);
    assert_eq!(readings[0].value, 9.5);
}
