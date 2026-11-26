use statsapi_socrata::decode_rows;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rows = decode_rows(include_bytes!("../tests/fixtures/cdc/places-tract.json"))?;

    println!("Socrata synthetic sample");
    println!("{:<18} {:<8} {:>10}", "Area", "Year", "Value");
    for row in rows.iter().take(3) {
        println!(
            "{:<18} {:<8} {:>10}",
            row["locationname"].as_str().unwrap_or("-"),
            row["year"].as_str().unwrap_or("-"),
            row["data_value"].as_str().unwrap_or("-")
        );
    }
    Ok(())
}
