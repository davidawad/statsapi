use statsapi_ons::{latest, Client};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let version = client.version("mid-year-pop-est", None)?;
    let order = client.options(&version, "time")?;
    let rows = client.observations(
        &version,
        &[
            ("time", "*"),
            ("geography", "E09000033"),
            ("age", "total"),
            ("sex", "all"),
        ],
    )?;
    if let Some(row) = latest(&rows, &order) {
        println!("{}: {}", row.time, row.value);
    }
    Ok(())
}
