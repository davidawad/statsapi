use statsapi_sdmx::parse_observation;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let eurostat = parse_observation(include_bytes!("../tests/fixtures/eurostat.json"))?;
    let csv = parse_observation(include_bytes!("../tests/fixtures/observations.csv"))?;
    let json = parse_observation(include_bytes!("../tests/fixtures/observations.json"))?;

    println!("SDMX synthetic sample");
    println!("{:<10} {:<12} {:>10}", "Format", "Period", "Value");
    print_observation("Eurostat", eurostat);
    print_observation("CSV", csv);
    print_observation("JSON", json);
    Ok(())
}

fn print_observation(format: &str, observation: (serde_json::Value, String)) {
    let (value, period) = observation;
    println!("{format:<10} {period:<12} {value:>10}");
}
