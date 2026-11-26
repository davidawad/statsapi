//! `CENSUS_API_KEY=... cargo run -p statsapi-census-bds --example basic`
use statsapi_census_bds::{Client, Geography};

fn main() -> Result<(), statsapi_census_bds::Error> {
    let client = Client::new(std::env::var("CENSUS_API_KEY").ok(), None)?;
    let o = client.newest(&Geography::State("10".into()))?;
    println!(
        "{} job creation rate: {:?}%",
        o.year,
        o.get("JOB_CREATION_RATE")
    );
    Ok(())
}
