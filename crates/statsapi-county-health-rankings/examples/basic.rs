use statsapi_county_health_rankings::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(None);
    let csv = client.fetch_annual_csv()?;
    if let Some(measure) = client.measure(&csv, "34023", "Premature Death")? {
        println!("Premature-death measure: {}", measure.value);
    }
    Ok(())
}
