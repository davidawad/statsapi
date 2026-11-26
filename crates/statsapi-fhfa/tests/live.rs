use statsapi_fhfa::{Client, Level};

#[test]
#[ignore = "live FHFA download; run once in the pod for source evidence"]
fn live_county_series_has_a_recent_year() -> Result<(), Box<dyn std::error::Error>> {
    let series = Client::new()?.series(Level::County, "34023")?;
    let latest = series.latest().ok_or("Middlesex NJ has no series")?;
    if latest.year < 2024 {
        return Err(format!("stale newest year: {latest:?}").into());
    }
    Ok(())
}
