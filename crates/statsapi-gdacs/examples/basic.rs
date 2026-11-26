use statsapi_gdacs::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let today = chrono::Utc::now().date_naive();
    let (alerts, origin) = Client::new().alerts(today - chrono::Duration::days(30), today)?;
    println!("{} orange/red alerts via {}", alerts.len(), origin.as_str());
    Ok(())
}
