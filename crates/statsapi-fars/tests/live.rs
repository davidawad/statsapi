use statsapi_fars::Client;

#[test]
#[ignore = "downloads the current national FARS archive"]
fn downloads_latest_published_archive() -> Result<(), Box<dyn std::error::Error>> {
    let data = Client::new().fetch_year(2024)?;
    if data.states().is_empty() {
        return Err("archive had no state rows".into());
    }
    Ok(())
}
