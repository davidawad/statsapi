use statsapi_sdmx::Client;

#[test]
#[ignore = "live OECD API smoke test"]
fn live_oecd_response_decodes() {
    if std::env::var_os("SDMX_API_KEY").is_some() {
        eprintln!("credential_missing: this public OECD endpoint does not consume SDMX_API_KEY");
    }
    let url = "https://sdmx.oecd.org/public/rest/data/OECD.SDD.STES,DSD_STES@DF_CLI/USA.M.LI...AA...H?startPeriod=2026-10&lastNObservations=1&dimensionAtObservation=AllDimensions&format=csvfile";
    let body = Client::new()
        .expect("client builds")
        .get(url)
        .expect("fetch OECD API");
    let (value, period) = statsapi_sdmx::parse_observation(&body).expect("response decodes");
    assert!(value.is_number());
    assert_eq!(period, "2026-10");
}
