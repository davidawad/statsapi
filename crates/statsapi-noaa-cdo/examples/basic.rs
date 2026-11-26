use statsapi_noaa_cdo::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let (start, end) = ("2026-10-01".parse()?, "2026-11-01".parse()?);
    let stations = client.stations_near(40.74, -74.17, 0.25, start, end)?;
    let days = client.daily_summaries(&stations[0].id, start, end, "PRCP,TMAX,TMIN")?;
    println!("{} days at {}", days.len(), stations[0].id);
    Ok(())
}
