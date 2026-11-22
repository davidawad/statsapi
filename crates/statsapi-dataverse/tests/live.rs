use statsapi_dataverse::Client;

#[test]
#[ignore = "live Dataverse API metadata request"]
fn live_reads_dataset_metadata() {
    if std::env::var_os("DATAVERSE_API_KEY").is_none() {
        println!("credential_missing: DATAVERSE_API_KEY is not configured");
        return;
    }
    let client = Client::new("https://dataverse.harvard.edu").expect("valid Dataverse URL");
    let metadata = client
        .get_json(
            "datasets/:persistentId/",
            &[("persistentId", "doi:10.7910/DVN/VOQCHQ")],
        )
        .expect("Dataverse dataset metadata is available");
    assert_eq!(
        metadata["status"], "OK",
        "Dataverse returns a successful API response"
    );
    assert!(
        metadata["data"]["latestVersion"]["files"].is_array(),
        "dataset files are listed"
    );
    println!("LIVE_EVIDENCE Dataverse dataset metadata returned; upstream values omitted");
}
