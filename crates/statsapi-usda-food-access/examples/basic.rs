use statsapi_usda_food_access::Client;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache_dir = std::env::var_os("USDA_FOOD_ACCESS_CACHE_DIR").map(PathBuf::from);
    let client = Client::new(cache_dir)?;
    let observation = client.atlas_value("01001020100", "lapop1")?;
    println!(
        "tract 01001020100: {} ({})",
        observation.value, observation.vintage
    );
    Ok(())
}
