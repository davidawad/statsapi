use statsapi_bis::{Client, Series};

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "the ignored live test uses assertions to verify source responses"
)]
#[ignore = "live BIS Data Portal request"]
fn fetches_us_series_live() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    for series in Series::ALL {
        let data = client.series(series, "US", 1)?;
        let latest = data.latest().ok_or("no observation")?;
        println!("{series:?} {} {} {}", latest.period, latest.value, data.url);
        assert!(latest.period.len() >= 6);
    }
    Ok(())
}
