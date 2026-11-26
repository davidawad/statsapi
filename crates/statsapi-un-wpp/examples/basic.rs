use statsapi_un_wpp::{WppClient, LAST_ESTIMATE_YEAR};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = WppClient::new()?;
    if let Some(rows) = client.fetch_country("USA")? {
        println!("{}", client.url());
        println!(
            "{:?}",
            rows.latest_value("MedianAgePop", LAST_ESTIMATE_YEAR)
        );
        println!("{:?}", rows.value(2050, "MedianAgePop"));
    }
    Ok(())
}
