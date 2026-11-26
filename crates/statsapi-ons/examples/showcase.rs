use serde_json::Value;
use statsapi_ons::{latest, Observation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let body: Value = serde_json::from_str(include_str!("../tests/fixtures/observations.json"))?;
    let observations: Vec<Observation> = body["observations"]
        .as_array()
        .ok_or("synthetic ONS response has no observations")?
        .iter()
        .filter_map(|row| {
            Some(Observation {
                time: row.pointer("/dimensions/Time/id")?.as_str()?.to_owned(),
                value: row["observation"].as_str()?.parse().ok()?,
                cv: row
                    .pointer("/metadata/CV")
                    .and_then(Value::as_str)
                    .and_then(|text| text.parse().ok()),
            })
        })
        .collect();
    let options: Value = serde_json::from_str(include_str!("../tests/fixtures/options.json"))?;
    let order: Vec<String> = options["items"]
        .as_array()
        .ok_or("synthetic ONS response has no options")?
        .iter()
        .filter_map(|item| item["option"].as_str().map(str::to_owned))
        .collect();
    let observation = latest(&observations, &order).ok_or("no synthetic ONS observation")?;
    let cv = observation
        .cv
        .map_or_else(|| "-".to_owned(), |value| format!("{value:.1}"));

    println!("ONS synthetic values");
    println!("{:<12} {:>12} {:>8}", "Period", "Value", "CV %");
    println!(
        "{:<12} {:>12.1} {:>8}",
        observation.time, observation.value, cv
    );
    Ok(())
}
