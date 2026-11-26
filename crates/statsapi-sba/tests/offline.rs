use statsapi_sba::{
    as_of_stamp, csv_links, decode_loans, decode_ppp, pick_resources, zip5, Client, Error, Program,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

const LOANS: &[u8] = include_bytes!("fixtures/loans-7a.csv");
const PPP: &[u8] = include_bytes!("fixtures/ppp.csv");

#[test]
fn decodes_loans_and_skips_rows_without_an_approval_date() {
    let mut loans = Vec::new();
    decode_loans(LOANS, |loan| loans.push(loan)).expect("fixture follows the FOIA layout");
    assert_eq!(loans.len(), 3);
    assert_eq!(loans[0].program, Program::SevenA);
    assert_eq!(loans[0].zip, "10001");
    assert!((loans[1].gross_approval - 250_000.5).abs() < f64::EPSILON);
    assert_eq!(loans[2].program, Program::FiveOhFour);
    assert_eq!(loans[2].naics_description, "Full-Service Restaurants");
}

#[test]
fn decodes_ppp_with_borrower_zip_fallback_and_bad_amounts() {
    let mut loans = Vec::new();
    decode_ppp(PPP, |loan| loans.push(loan)).expect("fixture follows the PPP layout");
    assert_eq!(loans.len(), 4);
    assert_eq!(loans[0].zip, "10001");
    assert_eq!(loans[0].state, "ZZ");
    assert_eq!(loans[1].zip, "10001");
    assert_eq!(loans[3].zip, "");
    assert!(loans[3].amount.abs() < f64::EPSILON);
}

#[test]
fn ppp_rows_with_non_utf8_bytes_still_decode() {
    let mut bytes =
        b"BorrowerZip,CurrentApprovalAmount,ProjectCountyName,ProjectState,ProjectZip\n".to_vec();
    bytes.extend_from_slice(b"10001,5,DO\xd1A ANA,ZZ,10001\n");
    let mut loans = Vec::new();
    decode_ppp(bytes.as_slice(), |loan| loans.push(loan)).expect("lossy decode");
    assert_eq!(loans.len(), 1);
    assert_eq!(loans[0].zip, "10001");
}

#[test]
fn missing_columns_are_named() {
    let error = decode_loans(&b"Program\n7A\n"[..], |_| ()).expect_err("columns validated");
    assert!(matches!(error, Error::MissingColumn("BorrZip")));
}

#[test]
fn discovers_files_from_dataset_page_links() {
    let page = r#"<a href="https://x/FOIA_7a_FY2020_Present_asof_300131.csv">a</a>
        <a href="https://x/FOIA_504_FY2010_Present_asof_300131.csv">b</a>
        <a href="https://x/FOIA_7a_FY2010_FY2019_asof_300131.csv">old</a>
        <a href="https://x/FOIA_7a_FY2020_Present_asof_300131.csv">dup</a>"#;
    let ppp_page =
        r#"<a href="https://x/public_150k_plus_240930.csv">p</a><a href="/dict.xlsx">d</a>"#;
    let links = csv_links(page);
    assert_eq!(links.len(), 3);
    let found = pick_resources(&links, &csv_links(ppp_page)).expect("files listed");
    assert!(found.seven_a.contains("FY2020_Present"));
    assert_eq!(as_of_stamp(&found.five_oh_four), Some("300131"));
    assert_eq!(found.ppp.len(), 1);
    assert!(matches!(
        pick_resources(&links, &[]),
        Err(Error::NoResource("PPP"))
    ));
}

#[test]
fn zip5_keeps_only_five_digit_prefixes() {
    assert_eq!(zip5(" 08840-1234 "), "08840");
    assert_eq!(zip5("N/A"), "");
    assert_eq!(zip5("123"), "");
}

fn serve_once(body: &'static [u8]) -> std::io::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body);
        }
    });
    Ok(format!("http://{address}/loans.csv"))
}

#[test]
fn client_streams_a_response_through_the_decoder() -> Result<(), Box<dyn std::error::Error>> {
    let url = serve_once(LOANS)?;
    let client = Client::new(Duration::from_secs(10))?;
    let mut count = 0;
    client.stream_loans(&url, |_| count += 1)?;
    if count != 3 {
        return Err(format!("expected 3 loans, streamed {count}").into());
    }
    Ok(())
}

#[test]
#[ignore = "live request to data.sba.gov; streams the 504 file (about 60 MB)"]
fn live_discovery_and_504_stream() {
    let client = Client::new(Duration::from_secs(300)).expect("client");
    let resources = client.discover().expect("dataset pages list the files");
    assert_ne!(resources.ppp.len(), 0);
    let mut count = 0_u64;
    client
        .stream_loans(&resources.five_oh_four, |_| count += 1)
        .expect("stream 504 file");
    assert!(count > 10_000, "504 file rows: {count}");
}
