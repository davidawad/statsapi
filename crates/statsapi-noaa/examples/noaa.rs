use statsapi_noaa::Client;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rows = Client::new().details(2024)?;
    println!("{} storm events", rows.len());
    Ok(())
}
