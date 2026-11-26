use statsapi_power_price::{Client, VINTAGES};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache = std::env::temp_dir().join("statsapi-power-price-example");
    let client = Client::new().with_cache_dir(cache);
    let rows = client.zip_rates(*VINTAGES.last().ok_or("no known OpenEI releases")?)?;
    println!("decoded {} ZIP-utility rows", rows.len());
    Ok(())
}
