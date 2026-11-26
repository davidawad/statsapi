use statsapi_urban_education::endpoint;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let payload: serde_json::Value =
        serde_json::from_slice(include_bytes!("../tests/fixtures/finance_page.json"))?;
    let row = payload["results"]
        .as_array()
        .and_then(|rows| rows.first())
        .ok_or("missing row")?;
    let url = endpoint("school-districts/ccd/finance/2024/", &[("fips", "99")]);

    println!("Urban Education synthetic sample");
    println!("{:<16} {:<12} {:>12}", "District", "Year", "Spending");
    println!(
        "{:<16} {:<12} {:>12.1}",
        row["leaid"].as_str().unwrap_or("-"),
        row["year"].as_i64().unwrap_or_default(),
        row["exp_total"].as_f64().unwrap_or_default()
    );
    println!("{:<16} {:<12} {:>12}", "Endpoint", "synthetic", url.len());
    Ok(())
}
