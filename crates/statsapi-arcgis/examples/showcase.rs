use statsapi_arcgis::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _client = Client::new().offline(true);
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/features.json"))?;
    let attributes = fixture
        .get("features")
        .and_then(|features| features.get(0))
        .and_then(|feature| feature.get("attributes"))
        .ok_or("ArcGIS fixture has no synthetic feature")?;
    let county = attributes["COUNTY"]
        .as_str()
        .ok_or("ArcGIS fixture has no county")?;

    println!("ArcGIS synthetic sample");
    println!("{:<12} {:>10}", "Field", "Value");
    println!("{:<12} {:>10}", "COUNTY", county);
    println!("{:<12} {:>10.1}", "RISK_SCORE", attributes["RISK_SCORE"]);
    Ok(())
}
