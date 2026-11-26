use statsapi_nasa_power::{Client, DailyQuery, Date};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let daily = Client::new().daily_point(&DailyQuery {
        latitude: 40.5434,
        longitude: -74.3626,
        start: Date::parse("20250101")?,
        end: Date::parse("20250107")?,
        parameters: &["T2M"],
    })?;
    println!("{:?}", daily.values("T2M"));
    Ok(())
}
