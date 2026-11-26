use statsapi_ioda::{Client, EntityType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let now = i64::try_from(now)?;
    let episodes = Client::new()?.episodes(EntityType::Country, "IQ", now - 30 * 86_400, now)?;
    println!("{} episodes in 30 days", episodes.len());
    Ok(())
}
