use statsapi_gleif::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = Client::new()?.count(&[("entity.legalAddress.country", "US")])?;
    println!("{} US legal entities", count.total);
    Ok(())
}
