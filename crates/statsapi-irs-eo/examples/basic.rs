use statsapi_irs_eo::{records, unzip_first, Client, PUB78_URL};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let archive = client.download(PUB78_URL)?;
    let csv = unzip_first(&archive)?;
    let (_, rows) = records(&csv)?;
    println!("{} Publication 78 records", rows.len());
    Ok(())
}
