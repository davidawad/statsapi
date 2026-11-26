use std::env;

use chrono::Utc;
use statsapi_wikidata_events::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let user_agent = env::var("WIKIMEDIA_USER_AGENT")?;
    let client = Client::new(&user_agent)?;
    let events = client.events("USA", Utc::now().date_naive())?;
    println!("{} Wikidata events", events.len());
    Ok(())
}
