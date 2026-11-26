use statsapi_ucdp_history::decode_csv;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rows = decode_csv(include_bytes!("../tests/fixtures/conflicts.csv"))?;

    println!("UCDP/PRIO synthetic sample");
    println!(
        "{:<12} {:>8} {:<18} {:>8}",
        "Conflict", "Year", "Sides", "Fatalities"
    );
    for row in &rows {
        println!(
            "{:<12} {:>8} {:<18} {:>8}",
            row["conflict_id"],
            row["year"],
            format!("{} / {}", row["side_a"], row["side_b"]),
            row["best"]
        );
    }
    Ok(())
}
