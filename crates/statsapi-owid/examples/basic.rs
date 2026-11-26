use statsapi_owid::OwidClient;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let observation = OwidClient::new()?.latest("life-expectancy", "USA")?;
    println!(
        "{} {} = {} ({}, via {})",
        observation.entity,
        observation.year,
        observation.value,
        observation.url,
        observation.path.as_str()
    );
    Ok(())
}
