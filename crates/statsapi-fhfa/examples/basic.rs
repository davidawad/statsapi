use statsapi_fhfa::{Client, Error, Level};

fn main() -> Result<(), Error> {
    let client = Client::new()?;
    if let Some(latest) = client.series(Level::County, "34023")?.latest() {
        println!(
            "Middlesex County NJ: HPI {} in {}, 5-year change {:?}%",
            latest.hpi, latest.year, latest.change_5y
        );
    }
    Ok(())
}
