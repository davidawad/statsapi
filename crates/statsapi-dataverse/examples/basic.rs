use statsapi_dataverse::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new("https://dataverse.harvard.edu")?;
    let dataset = client.get_json(
        "datasets/:persistentId/",
        &[("persistentId", "doi:10.7910/DVN/EXAMPLE")],
    )?;
    println!("{}", serde_json::to_string_pretty(&dataset)?);
    Ok(())
}
