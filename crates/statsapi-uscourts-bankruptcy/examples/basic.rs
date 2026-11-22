use statsapi_uscourts_bankruptcy::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let table = client.latest()?;
    println!("{}: {} county rows", table.date, table.counties.len());
    Ok(())
}
