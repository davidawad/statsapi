use statsapi_cms::decode_rows;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rows = decode_rows(include_str!("../tests/fixtures/geographic_variation.json"))?;
    let row = rows.first().ok_or("CMS fixture has no synthetic row")?;

    println!("CMS geographic variation synthetic sample");
    println!("{:<24} {:>12}", "Measure", "Value");
    println!(
        "{:<24} {:>12}",
        "Year",
        row.text("YEAR").ok_or("missing year")?
    );
    println!(
        "{:<24} {:>12.2}",
        "ER visits per 1,000",
        row.number("ER_VISITS_PER_1000_BENES")
            .ok_or("missing visits")?
    );
    Ok(())
}
