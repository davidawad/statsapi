use statsapi_bis::{Client, Series};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let data = client.series(Series::CreditGap, "US", 1)?;
    if let Some(latest) = data.latest() {
        println!("US credit-to-GDP gap {}: {}", latest.period, latest.value);
    }
    Ok(())
}
