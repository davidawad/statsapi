use chrono::NaiveDate;
use statsapi_cisa_kev::Catalog;
use std::io::Cursor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let catalog = Catalog::from_reader(Cursor::new(include_bytes!(
        "../tests/fixtures/catalog.json"
    )))?;
    let summary = catalog.summarize(NaiveDate::from_ymd_opt(2026, 11, 23).ok_or("bad date")?)?;

    println!("CISA KEV synthetic sample");
    println!("{:<24} {:>12}", "Measure", "Value");
    println!("{:<24} {:>12}", "Catalog entries", summary.catalog_size);
    println!("{:<24} {:>12}", "Added in 30 days", summary.added_30d);
    println!(
        "{:<24} {:>11.2}%",
        "Ransomware share", summary.ransomware_share
    );
    Ok(())
}
