#![expect(
    clippy::panic_in_result_fn,
    reason = "fixture assertions inside Result-returning tests"
)]
use statsapi_nasa_power::{Client, Daily, DailyQuery, Date};

#[test]
fn decodes_fixture_and_drops_fill_values() -> Result<(), Box<dyn std::error::Error>> {
    let body: serde_json::Value = serde_json::from_str(include_str!("fixtures/daily.json"))?;
    let daily = Daily::from_json("u", &body)?;
    let temps = daily.values("T2M");
    assert_eq!(temps.len(), 2);
    assert_eq!(temps[1].1.to_bits(), 4.0_f64.to_bits());
    assert_eq!(daily.units["PRECTOTCORR"], "mm/day");
    assert_eq!(
        daily.coordinates.map(|c| c.0.to_bits()),
        Some((-74.25_f64).to_bits())
    );
    assert_eq!(daily.values("MISSING").len(), 0);
    Ok(())
}

#[test]
fn rejects_unexpected_layout() {
    assert!(Daily::from_json("u", &serde_json::json!({"nope": 1})).is_err());
}

#[test]
fn date_arithmetic_crosses_leap_days_and_years() -> Result<(), Box<dyn std::error::Error>> {
    let date = Date::parse("2024-02-28")?;
    assert_eq!(date.add_days(2).iso(), "2024-03-01");
    assert_eq!(Date::parse("20260101")?.add_days(-1).compact(), "20251231");
    assert_eq!(Date::parse("19700101")?.to_days(), 0);
    assert!(Date::parse("20230229").is_err());
    assert!(Date::parse("2023-13-01").is_err());
    Ok(())
}

#[test]
fn builds_documented_url() -> Result<(), Box<dyn std::error::Error>> {
    let url = Client::new().url(&DailyQuery {
        latitude: 40.5434,
        longitude: -74.3626,
        start: Date::parse("20250101")?,
        end: Date::parse("20250102")?,
        parameters: &["T2M", "PRECTOTCORR"],
    });
    assert!(url.contains("parameters=T2M,PRECTOTCORR"));
    assert!(url.contains("longitude=-74.3626&latitude=40.5434"));
    assert!(url.ends_with("format=JSON"));
    Ok(())
}

#[test]
#[ignore = "queries the live NASA POWER API"]
fn live_daily_point() {
    let daily = Client::new()
        .daily_point(&DailyQuery {
            latitude: 40.5434,
            longitude: -74.3626,
            start: Date::parse("20250101").expect("date"),
            end: Date::parse("20250110").expect("date"),
            parameters: &["T2M"],
        })
        .expect("POWER should answer");
    assert_eq!(daily.values("T2M").len(), 10);
}
