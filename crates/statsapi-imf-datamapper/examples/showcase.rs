use serde_json::Value;
use statsapi_imf_datamapper::{horizon, parse_series};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let payload: Value =
        serde_json::from_slice(include_bytes!("../tests/fixtures/ngdp_rpch.json"))?;
    let series = parse_series(&payload, "NGDP_RPCH", "USA")?.ok_or("missing USA")?;
    let latest = horizon(&series, 2026, 0).ok_or("missing latest actual")?;

    println!("IMF DataMapper synthetic sample");
    println!("{:<12} {:>10} {:>10}", "Country", "Year", "Growth");
    println!("{:<12} {:>10} {:>10.1}", "USA", latest.year, latest.value);
    Ok(())
}
