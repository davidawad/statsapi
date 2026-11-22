use statsapi_seda::{Client, Dataset, Level};

fn main() -> Result<(), statsapi_seda::Error> {
    let client = Client::new()?.with_cache_dir("./seda-cache");
    let county = client.fetch(Level::County, Dataset::Achievement, "00001")?;
    println!(
        "{} matching rows from {} ({})",
        county.rows.len(),
        county.source_url,
        county.vintage
    );
    Ok(())
}
