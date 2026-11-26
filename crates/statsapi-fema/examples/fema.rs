use statsapi_fema::Client;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let response = Client::new().get_json("https://www.fema.gov/api/open/v3/NfipClaims?$top=1")?;
    println!("{response}");
    Ok(())
}
