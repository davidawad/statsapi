use statsapi_bea::{Client, Result};

fn main() -> Result<()> {
    let client = Client::from_env()?.with_cache_dir("./bea-cache");
    let observation = client.get_data("CAINC1", "3", "00000", "LAST5")?;
    println!("{} {}", observation.year, observation.value);
    Ok(())
}
