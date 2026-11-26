use std::io::Cursor;

use statsapi_fhfa::{parse_tract_csv, Latest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let series = parse_tract_csv(
        Cursor::new(include_bytes!("../tests/fixtures/tract.csv")),
        "01001020100",
    )?;
    let Latest {
        year,
        hpi,
        change_1y,
        ..
    } = series
        .latest()
        .ok_or("FHFA fixture has no synthetic series")?;

    println!("FHFA House Price Index synthetic values");
    println!(
        "{:<16} {:>8} {:>12} {:>12}",
        "Tract", "Year", "HPI", "Change 1y"
    );
    println!(
        "{:<16} {:>8} {:>12.1} {:>12.1}",
        "01001020100",
        year,
        hpi,
        change_1y.unwrap_or(0.0)
    );
    Ok(())
}
