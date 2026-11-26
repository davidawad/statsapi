use statsapi_bea::decode_observation;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (value, year) = decode_observation(
        include_bytes!("../tests/fixtures/regional-response.json"),
        "3",
    )?;

    println!("BEA synthetic sample");
    println!("{:<12} {:>12}", "Line", "Value");
    println!("{:<12} {:>12.0}", format!("3 ({year})"), value);
    Ok(())
}
