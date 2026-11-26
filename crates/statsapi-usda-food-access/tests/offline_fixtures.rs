use ephemeral_cache::{EphemeralCache, Source};
use statsapi_usda_food_access::{Client, Error};
use std::error::Error as StdError;

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions express the expected decoded fixture values"
)]
fn decodes_synthetic_ers_and_fns_fixtures_through_the_public_client(
) -> Result<(), Box<dyn StdError>> {
    let atlas = Client::decode_atlas_value(
        include_bytes!("fixtures/atlas.zip"),
        "99999000001",
        "lapop1",
    )?;
    assert_eq!((atlas.value, atlas.vintage.as_str()), (2468.0, "2019"));
    assert!(atlas.source_url.contains("ers.usda.gov/media/"));
    let state_persons = Client::decode_state_value(
        "persons",
        include_bytes!("fixtures/snap-persons.xlsx"),
        "AL",
    )?;
    assert_eq!(
        (state_persons.value, state_persons.vintage.as_str()),
        (2222.0, "February 2099")
    );
    let state_households = Client::decode_state_value(
        "households",
        include_bytes!("fixtures/snap-households.xlsx"),
        "AL",
    )?;
    assert_eq!(
        (state_households.value, state_households.vintage.as_str()),
        (444.0, "February 2099")
    );
    let county_persons =
        Client::decode_county_value(include_bytes!("fixtures/snap-county.zip"), "99999", 5)?;
    assert_eq!(
        (county_persons.value, county_persons.vintage.as_str()),
        (123.0, "July 2025")
    );
    let county_households =
        Client::decode_county_value(include_bytes!("fixtures/snap-county.zip"), "99999", 11)?;
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

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "assertions express the cache policy"
)]
fn fresh_extract_is_served_from_cache_and_no_archive_is_kept() -> Result<(), Box<dyn StdError>> {
    let dir = tempfile::tempdir()?;
    let url = "https://www.ers.usda.gov/media/5627/2019-large-retailer-access-map-lram-formerly-known-as-the-food-access-research-atlas-fara-data.zip?v=57565";
    EphemeralCache::bulk(dir.path())
        .put(&format!("{url}#atlas|99999000001|lapop1"), b"2468\t2019")?;
    let client = Client::new(Some(dir.path().to_owned()))?.offline_only();
    let atlas = client.atlas_value("99999000001", "lapop1")?;
    assert_eq!((atlas.value, atlas.source), (2468.0, Source::Cache));
    for entry in std::fs::read_dir(dir.path())? {
        let entry = entry?;
        assert!(entry.path().to_string_lossy().ends_with(".entry"));
        assert!(entry.metadata()?.len() < 64, "only the extract is stored");
    }
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the assertion checks the exact miss error"
)]
fn offline_without_a_cached_extract_is_a_miss() -> Result<(), Box<dyn StdError>> {
    let dir = tempfile::tempdir()?;
    let client = Client::new(Some(dir.path().to_owned()))?.offline_only();
    assert_eq!(
        client.county_value("99999", 5),
        Err(Error::from("offline cache miss"))
    );
    Ok(())
}
