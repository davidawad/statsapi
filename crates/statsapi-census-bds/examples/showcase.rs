use statsapi_census_bds::parse_newest;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let observation = parse_newest(include_str!("../tests/fixtures/state.json"))?;
    let jobs = observation
        .get("JOB_CREATION_RATE")
        .ok_or("BDS fixture has no job creation rate")?;
    let establishments = observation
        .get("ESTABS_ENTRY_RATE")
        .ok_or("BDS fixture has no establishment entry rate")?;

    println!("Census BDS synthetic sample");
    println!("{:<24} {:>12}", "Measure", "Value");
    println!("{:<24} {:>12.1}", "Year", observation.year);
    println!("{:<24} {:>12.1}", "Job creation rate", jobs);
    println!("{:<24} {:>12.1}", "Establishment entry", establishments);
    Ok(())
}
