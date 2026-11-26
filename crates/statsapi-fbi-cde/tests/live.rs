use statsapi_fbi_cde::{year_sum, Client, Error, Offense};

#[test]
#[ignore = "live FBI CDE API check"]
fn fetches_live_state_and_agency_summaries() {
    let client = match Client::from_env() {
        Ok(client) => client,
        Err(Error::MissingKey) => {
            println!("credential_missing: API_DATA_GOV_KEY");
            return;
        }
        Err(error) => panic!("client construction failed: {error}"),
    };
    let summary = client
        .state_summary("NJ", Offense::Violent, 2022, 2023)
        .expect("state summary");
    let (name, monthly) = summary.series_other_than(&[]).expect("state series");
    let rate = year_sum(monthly, 2023).expect("twelve months of 2023");
    assert!(rate > 0.0);
    println!("LIVE {name} 2023 violent crime per 100k = {rate:.1}");
    let agencies = client.agencies_by_state("NJ").expect("agency list");
    assert!(agencies.iter().any(|agency| agency.in_county("Middlesex")));
}
