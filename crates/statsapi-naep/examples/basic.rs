use statsapi_naep::{Client, Query};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let query = Query::new()
        .parameter("subject", "MAT")
        .parameter("grade", "4")
        .parameter("subscale", "MRPCM")
        .parameter("variable", "TOTAL")
        .parameter("jurisdiction", "CA")
        .parameter("stattype", "MN:MN")
        .parameter("Year", "Current");
    let response = Client::new().get(&query)?;
    println!("{}", serde_json::to_string_pretty(&response.result)?);
    Ok(())
}
