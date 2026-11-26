use std::env;

use statsapi_nara::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(env::var("NARA_API_KEY")?)?;
    let response = client.search("constitution", 10)?;
    println!(
        "NARA response keys: {}",
        response.as_object().map_or(0, serde_json::Map::len)
    );
    Ok(())
}
