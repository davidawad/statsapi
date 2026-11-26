use statsapi_ioda::{compare, Client, Datasource, EntityType};
use std::error::Error;

fn now() -> Result<i64, Box<dyn Error>> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    Ok(i64::try_from(secs)?)
}

#[test]
#[ignore = "queries the live IODA API"]
fn live_events_signals_and_regions_are_retrievable() -> Result<(), Box<dyn Error>> {
    let client = Client::new()?;
    let now = now()?;
    let episodes = client.episodes(EntityType::Country, "IQ", now - 30 * 86_400, now)?;
    println!("IQ episodes in 30d: {}", episodes.len());
    let signal = client.signal(
        EntityType::Country,
        "US",
        Datasource::Bgp,
        now - 8 * 86_400,
        now,
    )?;
    let cmp = compare(&signal).ok_or("no bgp comparison")?;
    println!("US bgp {}% of baseline", cmp.percent_of_baseline());
    if !(1.0..=500.0).contains(&cmp.percent_of_baseline()) {
        return Err("bgp ratio implausible".into());
    }
    let code = client
        .region_code("US", "California")?
        .ok_or("California region missing")?;
    println!("California region code {code}");
    Ok(())
}
