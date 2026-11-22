use statsapi_bls::{decode_ces, decode_laus};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let laus = decode_laus(include_bytes!("../tests/fixtures/laus.json"))?;
    let laus_observation = laus
        .get("TEST00000000000003")
        .and_then(|observation| observation.as_ref().ok())
        .ok_or("LAUS fixture has no synthetic monthly observation")?;
    let ces_observation = decode_ces(
        include_bytes!("../tests/fixtures/ces.json"),
        "SMU12000000000000001",
    )?;

    println!("BLS synthetic sample");
    println!("{:<24} {:<16} {:>12}", "Series", "Period", "Value");
    println!(
        "{:<24} {:<16} {:>12.1}",
        "LAUS TEST00000000000003", laus_observation.1, laus_observation.0
    );
    println!(
        "{:<24} {:<16} {:>12.1}",
        "CES SMU12000000000000001", ces_observation.1, ces_observation.0
    );
    Ok(())
}
