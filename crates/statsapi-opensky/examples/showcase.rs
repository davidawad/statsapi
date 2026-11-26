use serde_json::Value;
use statsapi_opensky::parse_snapshot;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let payload: Value = serde_json::from_str(include_str!("../tests/fixtures/states.json"))?;
    let snapshot = parse_snapshot(&payload)?;
    let ground = snapshot
        .on_ground_percent()
        .ok_or("empty synthetic snapshot")?;

    println!("OpenSky synthetic values");
    println!("{:<16} {:>12}", "Metric", "Value");
    println!("{:<16} {:>12}", "Total aircraft", snapshot.total);
    println!("{:<16} {:>12}", "Airborne", snapshot.airborne());
    println!("{:<16} {:>11.1}%", "On ground", ground);
    Ok(())
}
