use statsapi_epa_aqs::{numeric, response_rows};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let response: serde_json::Value =
        serde_json::from_slice(include_bytes!("../tests/fixtures/aqs-success.json"))?;
    let rows = response_rows(&response)?;
    let row = rows.first().ok_or("no synthetic AQS row")?;
    let mean = numeric(row, "arithmetic_mean").ok_or("AQS row has no mean")?;

    println!("EPA Air Quality System synthetic data");
    println!("{:<16} {:<12} {:>10}", "Measure", "Date", "Value");
    println!(
        "{:<16} {:<12} {:>10.1}",
        "PM2.5 mean",
        row["date_local"].as_str().unwrap_or("?"),
        mean
    );
    Ok(())
}
