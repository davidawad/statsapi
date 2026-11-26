use statsapi_hud_user::decode_json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fmr = decode_json(include_bytes!("../tests/fixtures/fmr.json"))?;
    let income = decode_json(include_bytes!("../tests/fixtures/income-limits.json"))?;
    let crosswalk = decode_json(include_bytes!("../tests/fixtures/zip-crosswalk.json"))?;
    let two_bedroom = fmr
        .pointer("/data/basicdata/Two-Bedroom")
        .ok_or("HUD FMR value missing")?;
    let median_income = income
        .pointer("/data/median_income")
        .ok_or("HUD income value missing")?;
    let rows = crosswalk
        .pointer("/data/results")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);

    println!("HUD USER housing data synthetic values");
    println!("{:<18} {:>12}", "Measure", "Value");
    println!("{:<18} {:>12}", "Two-bedroom FMR", two_bedroom);
    println!("{:<18} {:>12}", "Median income", median_income);
    println!("{:<18} {:>12}", "Crosswalk rows", rows);
    Ok(())
}
