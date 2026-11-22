use statsapi_usda_food_access::Client;
use std::error::Error;

#[test]
#[ignore = "downloads public USDA ERS and FNS data files"]
fn live_public_sources_are_retrievable_and_decodable() -> Result<(), Box<dyn Error>> {
    let client = Client::without_cache()?;
    let atlas = client.atlas_value("01001020100", "lapop1")?;
    let county = client.county_value("01001", 5)?;
    let state = client.state_value("persons", "AL")?;
    println!(
        "ERS Atlas: decoded public value {} ({})",
        atlas.value, atlas.vintage
    );
    println!(
        "FNS county: decoded public value {} ({})",
        county.value, county.vintage
    );
    println!(
        "FNS state: decoded public value {} ({})",
        state.value, state.vintage
    );
    Ok(())
}
