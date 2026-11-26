use std::io::{Cursor, Write};

use statsapi_fars::parse_archive;
use zip::{write::SimpleFileOptions, ZipWriter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    archive.start_file("synthetic/accident.csv", options)?;
    archive.write_all(include_bytes!("../tests/fixtures/accident.csv"))?;
    archive.start_file("synthetic/person.csv", options)?;
    archive.write_all(include_bytes!("../tests/fixtures/person.csv"))?;
    let archive = archive.finish()?.into_inner();
    let census = parse_archive(Cursor::new(archive))?;
    let totals = census.states().get("12").ok_or("no synthetic state")?;

    println!("FARS synthetic traffic fatalities");
    println!("{:<24} {:>10}", "Measure", "Value");
    println!("{:<24} {:>10}", "State fatalities", totals.fatalities);
    println!(
        "{:<24} {:>10}",
        "Alcohol impaired", totals.alcohol_impaired_fatalities
    );
    println!("{:<24} {:>10}", "Pedestrians", totals.pedestrian_fatalities);
    Ok(())
}
