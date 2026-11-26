use serde_json::Value;
use statsapi_usgs_earthquakes::{date_of, parse_first_event};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let json: Value = serde_json::from_slice(include_bytes!("../tests/fixtures/top_event.json"))?;
    let event = parse_first_event(&json)?.ok_or("earthquake fixture has no event")?;
    let title: String = event.title.chars().take(32).collect();

    println!("USGS earthquakes synthetic values");
    println!("ID           Date        Mag  Event");
    println!(
        "{:<12} {:<10} {:>4.1}  {title}",
        event.id,
        date_of(event.time_ms),
        event.magnitude
    );
    Ok(())
}
