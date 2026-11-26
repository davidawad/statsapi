use statsapi_usda_food_access::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let atlas = Client::decode_atlas_value(
        include_bytes!("../tests/fixtures/atlas.zip"),
        "99999000001",
        "lapop1",
    )?;
    let persons = Client::decode_state_value(
        "persons",
        include_bytes!("../tests/fixtures/snap-persons.xlsx"),
        "AL",
    )?;
    let households = Client::decode_state_value(
        "households",
        include_bytes!("../tests/fixtures/snap-households.xlsx"),
        "AL",
    )?;

    println!("USDA food access synthetic values");
    println!("Measure            Vintage          Value");
    println!(
        "{:<18} {:<16} {:>8.1}",
        "Atlas lapop1", atlas.vintage, atlas.value
    );
    println!(
        "{:<18} {:<16} {:>8.1}",
        "SNAP persons", persons.vintage, persons.value
    );
    println!(
        "{:<18} {:<16} {:>8.1}",
        "SNAP households", households.vintage, households.value
    );
    Ok(())
}
