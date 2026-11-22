use statsapi_worldbank::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(None)?;
    let response = client.indicator("USA", "SP.POP.TOTL", None)?;
    if let Some(observation) = Client::latest_observation(&response) {
        println!("{}: {}", observation["date"], observation["value"]);
    }
    Ok(())
}
