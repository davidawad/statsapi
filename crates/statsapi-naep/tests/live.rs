use statsapi_naep::{Client, Query};

fn query(stattype: &str) -> Query {
    Query::new()
        .parameter("subject", "MAT")
        .parameter("grade", "4")
        .parameter("subscale", "MRPCM")
        .parameter("variable", "TOTAL")
        .parameter("jurisdiction", "CA")
        .parameter("stattype", stattype)
        .parameter("Year", "Current")
}

#[test]
#[ignore = "live NAEP Data Service check; run once per source pod"]
fn live_mean_scale_score_response() {
    let query = query("MN:MN");
    let response = Client::new().get(&query).expect("NAEP responds");
    assert!(response
        .result
        .first()
        .is_some_and(|row| row["value"].is_number()));
    println!(
        "live NAEP mean scale-score response passed; url={}",
        query.url()
    );
}

#[test]
#[ignore = "live NAEP achievement-level Data Service check; run once per source pod"]
fn live_proficient_response() {
    let query = query("ALC:AP");
    let response = Client::new().get(&query).expect("NAEP responds");
    assert!(response
        .result
        .first()
        .is_some_and(|row| row["value"].is_number()));
    println!(
        "live NAEP achievement-level response passed; url={}",
        query.url()
    );
}
