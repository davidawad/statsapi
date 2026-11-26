use statsapi_county_health_rankings::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::default();
    let county = client
        .measure(
            include_bytes!("../tests/fixtures/annual.csv"),
            "34023",
            "Premature Death",
        )?
        .ok_or("county-health fixture has no synthetic measure")?;

    println!("County Health Rankings synthetic sample");
    println!("{:<24} {:>12}", "Measure", "Value");
    println!("{:<24} {:>12.2}", "Premature death", county.value);
    println!(
        "{:<24} {:>12.2}",
        "Confidence low",
        county.ci_low.ok_or("missing low")?
    );
    println!(
        "{:<24} {:>12.2}",
        "Confidence high",
        county.ci_high.ok_or("missing high")?
    );
    Ok(())
}
