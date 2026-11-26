use statsapi_meta_sci::decode_rows;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rows = decode_rows(
        include_bytes!("../tests/fixtures/country.csv").as_slice(),
        "AA",
    )?;
    let row = rows.first().ok_or("missing synthetic SCI row")?;

    println!("Meta SCI synthetic sample");
    println!("{:<16} {:<16} {:>10}", "Origin", "Friend", "Score");
    println!("{:<16} {:<16} {:>10.1}", "AA", row.friend, row.score);
    Ok(())
}
