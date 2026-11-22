use statsapi_hud_user::HudUserClient;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let token = std::env::var("HUD_API_TOKEN")?;
    let client = HudUserClient::builder(token)
        .cache_dir(std::env::temp_dir().join("hud-user-cache"))
        .build()?;
    let rents = client.fmr("3402399999", None)?;
    println!(
        "Two-bedroom FMR: {}",
        rents
            .pointer("/data/basicdata/Two-Bedroom")
            .unwrap_or(&serde_json::Value::Null)
    );
    let crosswalk = client.zip_crosswalk("08840", 1, Some(2025), Some(1))?;
    println!(
        "ZIP-to-tract rows: {}",
        crosswalk
            .pointer("/data/results")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len)
    );
    Ok(())
}
