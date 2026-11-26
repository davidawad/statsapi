use serde_json::Value;
use statsapi_worldbank::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let payload: Value =
        serde_json::from_slice(include_bytes!("../tests/fixtures/indicator.json"))?;
    let row = Client::latest_observation(&payload).ok_or("World Bank fixture has no value")?;

    println!("World Bank synthetic values");
    println!("Indicator       Date  Value");
    println!(
        "{:<15} {:>4}  {:>5}",
        row["indicator"]["id"].as_str().ok_or("missing indicator")?,
        row["date"].as_str().ok_or("missing date")?,
        row["value"].as_i64().ok_or("missing value")?
    );
    Ok(())
}
