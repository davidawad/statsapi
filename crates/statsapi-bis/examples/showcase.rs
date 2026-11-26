use statsapi_bis::parse_observations;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let observations = parse_observations(include_str!("../tests/fixtures/credit_gap.csv"))?;
    let newest = observations
        .last()
        .ok_or("BIS fixture has no observations")?;

    println!("BIS synthetic sample");
    println!("{:<16} {:>12}", "Period", "Gap");
    for observation in &observations {
        println!("{:<16} {:>12.2}", observation.period, observation.value);
    }
    println!("{:<16} {:>12.2}", "Newest", newest.value);
    Ok(())
}
