use serde_json::Value;
use statsapi_reliefweb::decode_disasters;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let payload: Value = serde_json::from_str(include_str!("../tests/fixtures/disasters.json"))?;
    let disasters = decode_disasters(&payload)?;

    println!("ReliefWeb synthetic values");
    println!("{:<12} {:<12} {:>12}", "Status", "Type", "Date");
    for disaster in disasters.iter().take(16) {
        println!(
            "{:<12} {:<12} {:>12}",
            disaster.status.as_str(),
            disaster.primary_type,
            disaster.date
        );
    }
    Ok(())
}
