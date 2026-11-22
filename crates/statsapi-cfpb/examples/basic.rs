use statsapi_cfpb::CfpbClient;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let client = CfpbClient::new(None)?;
    let response = client.complaints(
        "https://www.consumerfinance.gov/data-research/consumer-complaints/search/api/v1/?size=0&date_received_min=2025-01-01&date_received_max=2025-12-31&product=Credit%20card",
    )?;
    println!("matched complaints: {}", response.total);
    Ok(())
}
