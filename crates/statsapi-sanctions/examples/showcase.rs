use std::io::Cursor;

use statsapi_sanctions::{scan_ofac_sdn, scan_un, CountryNames, Date};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let names = CountryNames::for_alpha3("IRN").ok_or("unknown country")?;
    let today = Date::parse_iso("2026-11-23")?;
    let ofac = scan_ofac_sdn(
        Cursor::new(include_bytes!("../tests/fixtures/sdn.xml")),
        &names,
    )?;
    let un = scan_un(
        Cursor::new(include_bytes!("../tests/fixtures/un.xml")),
        &names,
        today,
    )?;

    println!("Sanctions synthetic sample");
    println!(
        "{:<8} {:>8} {:>10} {:>8} {:<24}",
        "List", "Entities", "People", "Recent", "Program"
    );
    println!(
        "{:<8} {:>8} {:>10} {:>8} {:<24}",
        "OFAC",
        ofac.exposure.entities,
        ofac.exposure.individuals,
        "-",
        ofac.exposure.program_titles(1)
    );
    println!(
        "{:<8} {:>8} {:>10} {:>8} {:<24}",
        "UN",
        un.entities,
        un.individuals,
        un.additions.unwrap_or(0),
        un.program_titles(1)
    );
    Ok(())
}
