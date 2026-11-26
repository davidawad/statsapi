use statsapi_ember::{latest, Dataset, EmberClient, Field};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = EmberClient::new()?;
    let (url, rows) = client.fetch_country(Dataset::Yearly, "USA")?;
    if let Some((row, share)) = latest(&rows, "Coal", Field::Share) {
        println!("{url}: coal {share}% of generation in {}", row.period);
    }
    Ok(())
}
