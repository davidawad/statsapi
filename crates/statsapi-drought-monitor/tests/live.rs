use statsapi_drought_monitor::{summarize, Area, Client, Date};
use std::error::Error;

#[test]
#[ignore = "queries the live US Drought Monitor service"]
fn live_county_and_state_weeks_are_retrievable() -> Result<(), Box<dyn Error>> {
    let client = Client::new()?;
    let end = Date::today();
    let start = end.plus_days(-400);
    for (area, aoi) in [(Area::County, "01001"), (Area::State, "01")] {
        let weeks = client.weekly(area, aoi, start, end)?;
        let summary = summarize(&weeks).ok_or("no weeks")?;
        println!(
            "{aoi}: {} d0={} d2={} 4w={:?} 1y={:?} weeks_d2={}",
            summary.latest.map_date,
            summary.latest.d0,
            summary.latest.d2,
            summary.d2_change_4w,
            summary.d2_change_1y,
            summary.weeks_d2_plus_1y
        );
        if !(0.0..=100.0).contains(&summary.latest.d0) {
            return Err("d0 outside 0..=100".into());
        }
    }
    Ok(())
}
