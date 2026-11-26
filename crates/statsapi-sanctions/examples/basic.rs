use statsapi_sanctions::{CountryNames, Date, SanctionsClient};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let names = CountryNames::for_alpha3("IRN").ok_or("unknown country")?;
    let today = Date::parse_iso("2026-11-23")?;
    let client = SanctionsClient::new()?;
    let un = client.un_exposure(&names, today)?;
    println!(
        "UN: {} entities, {} individuals, programs {}",
        un.entities,
        un.individuals,
        un.program_titles(5)
    );
    Ok(())
}
