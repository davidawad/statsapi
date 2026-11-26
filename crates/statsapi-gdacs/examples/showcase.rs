use statsapi_gdacs::decode_alerts;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let payload = serde_json::from_slice(include_bytes!("../tests/fixtures/alerts.json"))?;
    let alerts = decode_alerts(&payload)?;

    println!("GDACS disaster alerts synthetic values");
    println!("{:<8} {:>10} {:<8} {:<22}", "Event", "ID", "Level", "Name");
    for alert in alerts.iter().take(2) {
        println!(
            "{:<8} {:>10} {:<8} {:<22}",
            alert.event_type,
            alert.event_id,
            alert.level.as_str(),
            alert.name
        );
    }
    Ok(())
}
