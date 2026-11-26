use statsapi_vdem::{latest, Client};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let csv = client.fetch_extract()?;
    let value = latest(&csv, "USA", "v2x_libdem")?;
    println!("latest USA liberal democracy index: {value:?}");
    Ok(())
}
