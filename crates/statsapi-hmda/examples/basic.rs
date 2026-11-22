use statsapi_hmda::{Client, CsvRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder().build()?;
    let request = CsvRequest::new(2025, "counties", "01001");
    let applications = client.applications(&request)?;
    println!("{} applications", applications.len());
    Ok(())
}
