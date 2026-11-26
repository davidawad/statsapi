use statsapi_openaq::{nearest, plausible_mean, Client};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = std::env::var("OPENAQ_API_KEY")?;
    let client = Client::new(&key)?;
    let locations = client.locations_near(40.74, -74.17, 25_000)?;
    if let Some((location, distance_km)) = nearest(&locations, 40.74, -74.17) {
        let readings = client.location_latest(location.id)?;
        let mean = plausible_mean(readings.iter().map(|reading| reading.value));
        println!("{} ({distance_km:.1} km): {mean:?} PM2.5", location.name);
    }
    Ok(())
}
