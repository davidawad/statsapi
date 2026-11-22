use statsapi_usda_food_access::{Client, Error};
use std::error::Error as StdError;

fn seed_cache(
    directory: &tempfile::TempDir,
    url: &str,
    bytes: &[u8],
) -> Result<(), std::io::Error> {
    use sha2::{Digest, Sha256};
    let key = Sha256::digest(url.as_bytes());
    let filename = key.iter().fold(String::new(), |mut output, byte| {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
        output
    });
    std::fs::write(directory.path().join(format!("{filename}.body")), bytes)
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions express the expected decoded fixture values"
)]
fn decodes_synthetic_ers_and_fns_fixtures_through_the_public_client(
) -> Result<(), Box<dyn StdError>> {
    let cache = tempfile::tempdir()?;
    let client = Client::new(Some(cache.path().to_owned()))?;
    seed_cache(
        &cache,
        "https://www.ers.usda.gov/media/5627/2019-large-retailer-access-map-lram-formerly-known-as-the-food-access-research-atlas-fara-data.zip?v=57565",
        include_bytes!("fixtures/atlas.zip"),
    )?;
    seed_cache(
        &cache,
        "https://fns-prod.azureedge.us/sites/default/files/resource-files/snap-persons-11.xlsx",
        include_bytes!("fixtures/snap-persons.xlsx"),
    )?;
    seed_cache(
        &cache,
        "https://fns-prod.azureedge.us/sites/default/files/resource-files/snap-households-11.xlsx",
        include_bytes!("fixtures/snap-households.xlsx"),
    )?;
    seed_cache(
        &cache,
        "https://fns-prod.azureedge.us/sites/default/files/resource-files/snap-zip-fns388a-2.zip",
        include_bytes!("fixtures/snap-county.zip"),
    )?;
    let atlas = client.atlas_value("99999000001", "lapop1")?;
    assert_eq!((atlas.value, atlas.vintage.as_str()), (2468.0, "2019"));
    assert!(atlas.source_url.contains("ers.usda.gov/media/"));
    let state_persons = client.state_value("persons", "AL")?;
    assert_eq!(
        (state_persons.value, state_persons.vintage.as_str()),
        (2222.0, "February 2099")
    );
    let state_households = client.state_value("households", "AL")?;
    assert_eq!(
        (state_households.value, state_households.vintage.as_str()),
        (444.0, "February 2099")
    );
    let county_persons = client.county_value("99999", 5)?;
    assert_eq!(
        (county_persons.value, county_persons.vintage.as_str()),
        (123.0, "July 2025")
    );
    let county_households = client.county_value("99999", 11)?;
    assert_eq!(
        (county_households.value, county_households.vintage.as_str()),
        (45.0, "July 2025")
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the assertion checks the exact validation error"
)]
fn invalid_measure_is_rejected_without_network_access() -> Result<(), Box<dyn StdError>> {
    let client = Client::without_cache()?;
    assert_eq!(
        client.state_value("benefits", "AL"),
        Err(Error::from("measure must be persons or households"))
    );
    Ok(())
}
