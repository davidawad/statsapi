use statsapi_drought_monitor::{summarize, Area, Client, Date};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let end = Date::today();
    let weeks = Client::new()?.weekly(Area::County, "01001", end.plus_days(-400), end)?;
    println!("{:?}", summarize(&weeks));
    Ok(())
}
