use statsapi_noaa::parse_details;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let events = parse_details(include_bytes!("../tests/fixtures/details.csv"))?;

    println!("NOAA synthetic values");
    println!("{:<14} {:<12} {:>12}", "County", "Event", "Property $");
    for event in events.iter().take(16) {
        println!(
            "{:<14} {:<12} {:>12.0}",
            event.county, event.event_type, event.property_damage
        );
    }
    Ok(())
}
