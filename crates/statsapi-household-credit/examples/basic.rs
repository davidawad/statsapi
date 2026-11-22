use statsapi_household_credit::{county_debt_to_income_bound, Client, CountyBound};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Supply a directory to cache downloaded responses; omit it for memory-only access.
    let client = Client::new()?;
    let archive = client.county_archive()?;
    let latest = county_debt_to_income_bound(&archive, "34023", CountyBound::Low)
        .map_err(std::io::Error::other)?;
    println!("latest interval: {latest:?}");
    Ok(())
}
