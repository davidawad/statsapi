use statsapi_census::parse_acs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rows = parse_acs(include_bytes!("../tests/fixtures/acs.json"))?;
    let county = rows
        .get("Synthetic County")
        .ok_or("Census fixture has no synthetic county")?;
    let (estimate, margin) = county
        .get("B01003_001")
        .ok_or("Census fixture has no population estimate")?;

    println!("Census ACS synthetic sample");
    println!("{:<24} {:>12}", "Measure", "Value");
    println!(
        "{:<24} {:>12.0}",
        "Population estimate",
        estimate.ok_or("missing estimate")?
    );
    println!(
        "{:<24} {:>12.0}",
        "Margin of error",
        margin.ok_or("missing margin")?
    );
    Ok(())
}
