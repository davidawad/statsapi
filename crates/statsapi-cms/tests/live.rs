use statsapi_cms::{Client, Level};

#[test]
#[ignore = "live data.cms.gov check"]
fn fetches_newest_county_rows_from_both_datasets() {
    let client = Client::default();
    let variation = client
        .geographic_variation(Level::County, "34023")
        .expect("geographic variation request")
        .expect("Middlesex County row");
    assert!(variation.number("TOT_MDCR_PYMT_PC").is_some());
    let opioid = client
        .opioid_prescribing(Level::County, "34023")
        .expect("opioid request")
        .expect("Middlesex County opioid row");
    assert!(opioid.number("Opioid_Prscrbng_Rate").is_some());
    println!(
        "{} per-capita ${:?}; opioid {:?}%",
        variation.text("YEAR").unwrap_or("?"),
        variation.number("TOT_MDCR_PYMT_PC"),
        opioid.number("Opioid_Prscrbng_Rate")
    );
}
