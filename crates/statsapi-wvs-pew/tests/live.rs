use statsapi_wvs_pew::{pew_aggregate, weighted_wvs, WvsMetric};
use std::path::PathBuf;

#[test]
#[ignore = "requires a manually downloaded WVS file and WVS_LOGIN"]
fn live_wvs_manual_file_smoke() {
    if !std::env::var("WVS_LOGIN").is_ok_and(|value| !value.trim().is_empty()) {
        println!("credential_missing: WVS_LOGIN");
        return;
    }
    let Some(root) = std::env::var_os("WVS_DATA_DIR") else {
        println!("credential_missing: WVS_DATA_DIR");
        return;
    };
    let bytes = std::fs::read(PathBuf::from(root).join("wave7.csv")).expect("manual WVS file");
    let result =
        weighted_wvs(&bytes, "USA", WvsMetric::GeneralizedTrust, false).expect("valid WVS CSV");
    assert!(result.is_some(), "WVS file has no USA trust estimate");
}

#[test]
#[ignore = "requires a manually prepared Pew aggregate file and PEW_LOGIN"]
fn live_pew_manual_file_smoke() {
    if !std::env::var("PEW_LOGIN").is_ok_and(|value| !value.trim().is_empty()) {
        println!("credential_missing: PEW_LOGIN");
        return;
    }
    let Some(root) = std::env::var_os("PEW_DATA_DIR") else {
        println!("credential_missing: PEW_DATA_DIR");
        return;
    };
    let bytes = std::fs::read(PathBuf::from(root).join("aggregates.csv"))
        .expect("manually prepared Pew file");
    let result = pew_aggregate(&bytes, "USA", "youtube_use", None).expect("valid Pew CSV");
    assert!(result.is_some(), "Pew file has no USA YouTube aggregate");
}
