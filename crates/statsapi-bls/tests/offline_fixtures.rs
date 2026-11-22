use statsapi_bls::{decode_ces, decode_laus, decode_qcew};

#[test]
fn decodes_upstream_shaped_synthetic_fixtures() {
    let laus = decode_laus(include_bytes!("fixtures/laus.json")).expect("synthetic LAUS fixture");
    let observation = laus["TEST00000000000003"]
        .as_ref()
        .expect("monthly observation");
    assert_eq!(observation.0, 8.7);
    assert_eq!(observation.1, "2099 January");

    let qcew = decode_qcew(
        include_bytes!("fixtures/qcew.csv"),
        "99999",
        "fixture-url".to_owned(),
    )
    .expect("synthetic QCEW fixture");
    assert_eq!(qcew.0, 1234.0);
    assert_eq!(qcew.2, "fixture-url");

    let ces = decode_ces(include_bytes!("fixtures/ces.json"), "SMU12000000000000001")
        .expect("synthetic CES fixture");
    assert_eq!(ces.0, 123_000.0);
}
