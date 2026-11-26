use statsapi_imf_datamapper::{horizon, Client};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    if let Some(series) = client.country_series("NGDP_RPCH", "USA")? {
        if let Some(latest) = horizon(&series, 2026, 0) {
            println!("{}: {}", latest.year, latest.value);
        }
    }
    Ok(())
}
