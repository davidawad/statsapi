use statsapi_urban_education::{Client, Options, API_ROOT};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::with_options(Options::default())?;
    let url = format!("{API_ROOT}/school-districts/ccd/directory/2024/?fips=34&page_size=5");
    for row in client.rows(&url)? {
        println!("{row}");
    }
    Ok(())
}
