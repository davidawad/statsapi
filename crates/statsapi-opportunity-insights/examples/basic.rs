use statsapi_opportunity_insights::{Client, Dataset};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let table = client.fetch(Dataset::SocialCapitalZip)?;
    println!("{} records", table.rows().len());
    Ok(())
}
