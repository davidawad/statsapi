use std::env;

use statsapi_fcc_bdc::{Client, Credentials};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let credentials = Credentials::new(
        env::var("FCC_BDC_USERNAME")?,
        env::var("FCC_BDC_HASH_VALUE")?,
    )
    .ok_or("FCC_BDC_USERNAME and FCC_BDC_HASH_VALUE are required")?;
    let client = Client::new(credentials)?;
    let availability = client.county("06037")?;
    println!("{}: {} units", availability.as_of, availability.total_units);
    Ok(())
}
