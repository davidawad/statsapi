use serde_json::Value;
use statsapi_who_gho::latest_observation;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let payload: Value =
        serde_json::from_slice(include_bytes!("../tests/fixtures/suicide-usa.json"))?;
    let row = latest_observation(&payload).ok_or("WHO fixture has no observation")?;
    let indicator = row["IndicatorCode"].as_str().ok_or("missing indicator")?;
    let year = row["TimeDim"].as_i64().ok_or("missing year")?;
    let value = row["NumericValue"].as_f64().ok_or("missing value")?;

    println!("WHO GHO synthetic values");
    println!("Indicator              Year  Value");
    println!("{indicator:<22} {year:>4}  {value:>5.1}");
    Ok(())
}
