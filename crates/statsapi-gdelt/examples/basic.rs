use statsapi_gdelt::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let timeline = client.timeline(
        "\"Paris\" sourcecountry:FR",
        "timelinevolraw",
        "20261101",
        "20261122",
    )?;
    for point in timeline {
        println!("{} {}", point.date, point.value);
    }
    Ok(())
}
