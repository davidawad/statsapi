use statsapi_cms::{Client, Level};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::default();
    if let Some(row) = client.geographic_variation(Level::State, "34")? {
        println!(
            "NJ {} Medicare ER visits per 1,000: {:?}",
            row.text("YEAR").unwrap_or("?"),
            row.number("ER_VISITS_PER_1000_BENES")
        );
    }
    Ok(())
}
