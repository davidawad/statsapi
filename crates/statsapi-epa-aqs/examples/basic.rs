use std::env;

use statsapi_epa_aqs::{response_rows, Client, Credentials};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let credentials = Credentials::new(env::var("EPA_AQS_EMAIL")?, env::var("EPA_AQS_KEY")?)?;
    let client = Client::new(credentials)?;
    let query = client.query("pm25_annual_mean", "county", "06037", None, None, 2024)?;
    let response = client.fetch(&query)?;
    println!("{} AQS rows", response_rows(&response)?.len());
    Ok(())
}
