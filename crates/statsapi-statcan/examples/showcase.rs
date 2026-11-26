use statsapi_statcan::{pad_coordinate, province_sgc, Observation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let payload: serde_json::Value =
        serde_json::from_slice(include_bytes!("../tests/fixtures/data.json"))?;
    let point = &payload[0]["object"]["vectorDataPoint"][0];
    let observation = Observation {
        vector_id: payload[0]["object"]["vectorId"]
            .as_u64()
            .ok_or("missing vector")?,
        ref_period: point["refPer"]
            .as_str()
            .ok_or("missing period")?
            .to_string(),
        value: point["value"].as_f64().ok_or("missing value")?,
        release_time: point["releaseTime"].as_str().map(str::to_string),
    };
    let coordinate = pad_coordinate("4.1");

    println!("Statistics Canada synthetic sample");
    println!("{:<16} {:<18} {:>12}", "Series", "Period", "Value");
    println!(
        "{:<16} {:<18} {:>12.1}",
        format!("Vector {}", observation.vector_id),
        observation.ref_period,
        observation.value
    );
    println!("{:<16} {:<18} {:>12}", "Coordinate", coordinate, "- ");
    println!(
        "{:<16} {:<18} {:>12}",
        "Province SGC",
        province_sgc("ON").unwrap_or("-"),
        "-"
    );
    Ok(())
}
