use serde_json::Value;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let payload: Value =
        serde_json::from_slice(include_bytes!("../tests/fixtures/collection.json"))?;
    let claims = payload
        .get("NfipClaims")
        .and_then(Value::as_array)
        .ok_or("FEMA fixture has no synthetic claims")?;
    let zip = claims
        .first()
        .and_then(|claim| claim.get("reportedZipCode"))
        .and_then(Value::as_str)
        .ok_or("FEMA fixture claim has no ZIP")?;
    let count = payload
        .pointer("/metadata/count")
        .and_then(Value::as_u64)
        .ok_or("FEMA fixture has no count")?;

    println!("OpenFEMA synthetic values");
    println!("{:<20} {:>12} {:>12}", "Collection", "ZIP", "Records");
    println!("{:<20} {:>12} {:>12}", "NfipClaims", zip, count);
    Ok(())
}
