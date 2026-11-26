use statsapi_governance_indices::parse_rsf_csv;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let edition = parse_rsf_csv(
        include_bytes!("../tests/fixtures/rsf-2026.csv"),
        2026,
        "synthetic://rsf-2026.csv",
    )?;
    let observation = edition
        .country("USA")
        .ok_or("governance fixture has no synthetic USA row")?;

    println!("Governance indices synthetic values");
    println!(
        "{:<12} {:>10} {:>8} {:>12}",
        "Index", "Country", "Score", "Rank"
    );
    println!(
        "{:<12} {:>10} {:>8.2} {:>12}",
        "RSF 2026",
        observation.iso3,
        observation.score,
        observation.rank.unwrap_or_default()
    );
    Ok(())
}
