use serde_json::Value;
use statsapi_openaq::{nearest, parse_days, parse_latest, parse_locations, plausible_mean};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let locations: Value = serde_json::from_str(include_str!("../tests/fixtures/locations.json"))?;
    let locations = parse_locations(&locations)?;
    let latest: Value = serde_json::from_str(include_str!("../tests/fixtures/latest.json"))?;
    let readings = parse_latest(&latest)?;
    let days: Value = serde_json::from_str(include_str!("../tests/fixtures/days.json"))?;
    let days = parse_days(&days)?;
    let (location, distance) = nearest(&locations, 40.71, -74.0).ok_or("no location")?;
    let reading = readings
        .iter()
        .find(|reading| reading.sensor == location.pm25_sensor)
        .ok_or("no PM2.5 reading")?;
    let mean = plausible_mean(days.iter().map(|day| day.value)).ok_or("no daily mean")?;

    println!("OpenAQ synthetic values");
    println!("{:<18} {:>12}", "Metric", "Value");
    println!("{:<18} {:>12.1}", "Nearest km", distance);
    println!("{:<18} {:>12.1}", "Latest PM2.5", reading.value);
    println!("{:<18} {:>12.1}", "Daily mean", mean);
    Ok(())
}
