use statsapi_wikidata_events::parse_events;

#[test]
fn parses_synthetic_event_fixture() {
    let events = parse_events(
        b"Event Date,Country,Admin1,Event Description,Event Type,Fatalities\n2026-01-01,FRA,Provence,Protest,Demonstration,2\n",
    )
    .expect("fixture parses");
    assert_eq!(events[0].country, "FRA");
    assert_eq!(events[0].fatalities, Some(2));
}
