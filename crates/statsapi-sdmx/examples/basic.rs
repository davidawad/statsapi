use statsapi_sdmx::{Agency, Client};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = statsapi_sdmx::data_url(Agency::Ecb, None, "ICP", "M.FR.N.000000.4.ANR", None);
    let client = Client::builder()
        .cache_dir(std::env::temp_dir().join("statsapi-sdmx-cache"))
        .build()?;
    let response = client.get(&url)?;
    let (value, period) = statsapi_sdmx::parse_observation(&response)?;
    println!("ECB observation for {period}: {value}");
    Ok(())
}
