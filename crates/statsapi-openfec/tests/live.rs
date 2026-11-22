use statsapi_openfec::Client;

#[test]
#[ignore = "live OpenFEC API check"]
fn fetches_live_aggregate_and_committee_data() {
    let client = match Client::from_env() {
        Ok(client) => client,
        Err(statsapi_openfec::Error::MissingKey) => {
            println!("credential_missing: API_DATA_GOV_KEY");
            return;
        }
        Err(error) => panic!("client construction failed: {error}"),
    };
    let result = client
        .schedule_a_by_zip("00000", 2024)
        .expect("OpenFEC ZIP aggregate request succeeds");
    assert!(result.committees.iter().all(|committee| {
        !committee.committee_id.is_empty()
            && committee
                .committee_type
                .as_deref()
                .is_some_and(|kind| matches!(kind, "H" | "S" | "P"))
    }));
    println!(
        "LIVE OpenFEC ZIP aggregate total={} committee_rows={}",
        result.total,
        result.committees.len()
    );
}
