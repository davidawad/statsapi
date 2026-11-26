use statsapi_nara::decode_response;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let response = decode_response(include_bytes!("../tests/fixtures/catalog.json"))?;
    let records = response["opaResponse"]["results"]
        .as_array()
        .ok_or("missing synthetic records")?;

    println!("National Archives Catalog synthetic sample");
    println!("{:<16} {:>10} {:<18}", "Measure", "Value", "Unit");
    println!("{:<16} {:>10} {:<18}", "Records", records.len(), "count");
    println!(
        "{:<16} {:>10} {:<18}",
        "Title length",
        records[0]["title"].as_str().ok_or("missing title")?.len(),
        "characters"
    );
    Ok(())
}
