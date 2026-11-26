use statsapi_uscourts_bankruptcy::parse_xlsx;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let table = parse_xlsx(
        include_bytes!("../tests/fixtures/f5a-synthetic.xlsx"),
        "synthetic fixture",
        "2030-03-31",
    )?;
    let (fips, county) = table
        .counties
        .iter()
        .next()
        .ok_or("bankruptcy fixture has no county rows")?;

    println!("US Courts bankruptcy synthetic values");
    println!("FIPS  County                   All   Ch7  Ch13");
    println!(
        "{fips:<5} {:24.24} {:>5} {:>5} {:>5}",
        county.name,
        value(county.all),
        value(county.chapter7),
        value(county.chapter13)
    );
    Ok(())
}

fn value(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| format!("{value:.0}"))
}
