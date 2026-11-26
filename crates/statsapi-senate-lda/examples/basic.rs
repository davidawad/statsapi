use statsapi_senate_lda::{last_four_quarters, FilingQuery, LdaClient};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = LdaClient::new()?;
    let mut total = 0;
    for period in last_four_quarters("2026-11-23")? {
        total += client.count(&FilingQuery::new(period).client_state("NJ")?)?;
    }
    println!("NJ client filings, last four quarters: {total}");
    Ok(())
}
