use statsapi_owid::latest_in_csv;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let observation = latest_in_csv(
        include_bytes!("../tests/fixtures/filtered.csv").as_slice(),
        "USA",
        "synthetic",
    )?;

    println!("OWID synthetic values");
    println!("{:<12} {:>8} {:>12}", "Country", "Year", "Value");
    println!(
        "{:<12} {:>8} {:>12.1}",
        observation.code, observation.year, observation.value
    );
    Ok(())
}
