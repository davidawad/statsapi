use statsapi_eurostat::Observation;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let payload: serde_json::Value =
        serde_json::from_slice(include_bytes!("../tests/fixtures/gdp-nuts2.json"))?;
    let value = payload["value"]["0"].as_f64().ok_or("no synthetic value")?;
    let observation = Observation {
        time: "2023".into(),
        value,
        flag: payload["status"]["0"].as_str().map(str::to_owned),
        geo_label: Some("Oberbayern".into()),
        dataset_label: payload["label"].as_str().map(str::to_owned),
        updated: payload["updated"].as_str().map(str::to_owned),
    };

    println!("Eurostat synthetic GDP");
    println!("{:<16} {:<10} {:>12}", "Region", "Period", "Value");
    println!(
        "{:<16} {:<10} {:>12.0}",
        observation.geo_label.as_deref().unwrap_or("?"),
        observation.time,
        observation.value
    );
    Ok(())
}
