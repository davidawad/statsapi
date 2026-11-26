use statsapi_senate_lda::{last_four_quarters, FilingQuery, LdaClient};

#[test]
#[ignore = "live Senate LDA API smoke test"]
fn live_counts_new_jersey_client_filings_for_the_latest_quarter() {
    let periods = last_four_quarters("2026-11-23").expect("periods");
    let query = FilingQuery::new(*periods.last().expect("a period"))
        .client_state("NJ")
        .expect("state");
    let count = LdaClient::new()
        .expect("client")
        .count(&query)
        .expect("live count");
    assert!(count > 0, "no NJ client filings for {}", query.period());
}
