use statsapi_ember::{latest, parse_country_rows, Field};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rows = parse_country_rows(
        &include_bytes!("../tests/fixtures/monthly-usa.csv")[..],
        "USA",
    )?;
    let (row, share) = latest(&rows, "Coal", Field::Share).ok_or("no synthetic coal row")?;

    println!("Ember synthetic electricity generation");
    println!("{:<16} {:<12} {:>10}", "Source", "Period", "Share %");
    println!("{:<16} {:<12} {:>10.3}", "Coal", row.period, share);
    Ok(())
}
