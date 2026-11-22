use serde_json::Value;
use statsapi_socrata::{decode_rows, Error, SocrataClient};

#[test]
fn decodes_synthetic_cdc_shaped_fixtures_without_upstream_values() {
    let places =
        decode_rows(include_bytes!("fixtures/cdc/places-tract.json")).expect("places rows");
    assert_eq!(places[0]["locationid"], "00000000000");
    assert_eq!(places[0]["data_value"], "41.7");

    let state = decode_rows(include_bytes!("fixtures/cdc/vsrr-state.json")).expect("state rows");
    assert_eq!(state[0]["period"], "12 month-ending");
    assert_eq!(state[0]["data_value"], "812");

    let county = decode_rows(include_bytes!("fixtures/cdc/vsrr-county.json")).expect("county rows");
    assert_eq!(county[0]["end_date"], "2025-11-30T00:00:00.000");
    assert_eq!(county[0]["provisional_drug_overdose"], "27");
}

#[test]
fn builds_encoded_resource_query_and_optional_token_client() {
    let url = SocrataClient::resource_url(
        "data.cdc.gov",
        "synthetic-id",
        &[
            ("where".into(), "name='Synthetic'".into()),
            ("limit".into(), "1".into()),
        ],
    )
    .expect("valid resource URL");
    assert!(url.starts_with("https://data.cdc.gov/resource/synthetic-id.json?"));
    assert!(url.contains("%27Synthetic%27"));
    SocrataClient::builder()
        .app_token("sample-token")
        .build()
        .expect("token client");
}

#[test]
fn rejects_non_array_json() {
    assert!(matches!(
        decode_rows(br#"{"row":1}"#),
        Err(Error::InvalidResponse)
    ));
}

#[test]
#[ignore = "live public CDC SODA endpoint"]
fn live_cdc_rows() {
    let client = match std::env::var("SOCRATA_APP_TOKEN") {
        Ok(token) if !token.trim().is_empty() => SocrataClient::builder().app_token(token).build(),
        _ => {
            eprintln!("credential_missing: SOCRATA_APP_TOKEN (anonymous read is supported)");
            SocrataClient::new()
        }
    }
    .expect("construct live client");
    let url = SocrataClient::resource_url(
        "data.cdc.gov",
        "cwsq-ngmh",
        &[
            ("limit".into(), "1".into()),
            ("select".into(), "locationid".into()),
        ],
    )
    .expect("live query URL");
    let rows: Vec<Value> = client.get_rows(&url).expect("live CDC response");
    assert_eq!(rows.len(), 1);
}
