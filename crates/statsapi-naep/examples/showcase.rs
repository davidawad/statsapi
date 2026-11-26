use sha2::{Digest, Sha256};
use statsapi_naep::{Client, Query};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let query = Query::new()
        .parameter("subject", "MAT")
        .parameter("grade", "4")
        .parameter("subscale", "MRPCM")
        .parameter("variable", "TOTAL")
        .parameter("jurisdiction", "CA")
        .parameter("stattype", "MN:MN")
        .parameter("Year", "Current");
    let dir = std::env::temp_dir().join(format!("statsapi-naep-showcase-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    let digest = Sha256::digest(query.url().as_bytes());
    fs::write(
        dir.join(format!("{digest:x}")),
        include_bytes!("../tests/fixtures/data-response.json"),
    )?;
    let response = Client::new()
        .with_cache_dir(&dir)
        .offline(true)
        .get(&query)?;
    let value = response
        .result
        .first()
        .and_then(|row| row["value"].as_f64());
    println!("NAEP synthetic sample");
    println!("{:<16} {:>10} {:<10}", "Measure", "Value", "Unit");
    println!(
        "{:<16} {:>10.1} {:<10}",
        "Mean score",
        value.ok_or("missing score")?,
        "points"
    );
    let _ = fs::remove_dir_all(dir);
    Ok(())
}
