use statsapi_cow::decode_csv;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rows = decode_csv(include_bytes!("../tests/fixtures/country-codes.csv"))?;

    println!("COW synthetic country codes");
    println!("{:<8} {:>8} {:>10}", "Code", "Start", "End");
    for row in &rows {
        println!(
            "{:<8} {:>8} {:>10}",
            row.get("ccode").ok_or("missing code")?,
            row.get("styear").ok_or("missing start")?,
            row.get("stendyear").ok_or("missing end")?
        );
    }
    Ok(())
}
