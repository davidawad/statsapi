use statsapi_wikidata_events::parse_events;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let events = parse_events(include_bytes!("../tests/fixtures/showcase.csv"))?;

    println!("Wikidata events synthetic values");
    println!("Date        Country Type       Fatal  Event");
    for event in events.iter().take(2) {
        let title: String = event.title.chars().take(20).collect();
        let fatalities = event
            .fatalities
            .map_or_else(|| "-".to_owned(), |value| value.to_string());
        println!(
            "{}  {:<7} {:<10} {:>5}  {title}",
            event.date, event.country, event.kind, fatalities
        );
    }
    Ok(())
}
