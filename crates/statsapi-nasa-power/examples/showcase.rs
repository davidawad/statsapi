use statsapi_nasa_power::Daily;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let body: serde_json::Value =
        serde_json::from_slice(include_bytes!("../tests/fixtures/daily.json"))?;
    let daily = Daily::from_json("synthetic", &body)?;
    let temperature = daily.values("T2M");
    let precipitation = daily.values("PRECTOTCORR");

    println!("NASA POWER synthetic sample");
    println!("{:<16} {:>10} {:<10}", "Parameter", "Days", "Unit");
    println!("{:<16} {:>10} {:<10}", "T2M", temperature.len(), "C");
    println!(
        "{:<16} {:>10} {:<10}",
        "PRECTOTCORR",
        precipitation.len(),
        "mm/day"
    );
    Ok(())
}
