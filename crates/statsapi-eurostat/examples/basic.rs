use statsapi_eurostat::{Client, NutsLevel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    if let Some(region) = client.nuts_for_point(48.1371, 11.5754, NutsLevel::Two, 2024)? {
        let filters = [("geo", region.id.as_str()), ("unit", "PPS_EU27_2020_HAB")];
        if let Some(observation) = client.latest("nama_10r_2gdp", &filters)? {
            println!(
                "{} {}: {}",
                region.name, observation.time, observation.value
            );
        }
    }
    Ok(())
}
