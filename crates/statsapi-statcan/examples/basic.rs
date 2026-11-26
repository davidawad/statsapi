use statsapi_statcan::{Client, CANADA_CODE};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    // Table 14-10-0287-01: unemployment rate, 15+, both genders, seasonally adjusted.
    let (geo, _, observation) =
        client.latest_for_geography(14_100_287, "{geo}.7.1.1.1.1", CANADA_CODE)?;
    println!(
        "{}: {} ({})",
        geo.name, observation.value, observation.ref_period
    );
    Ok(())
}
