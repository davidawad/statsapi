//! Streams the 504 loan file and counts approvals for one ZIP.
use statsapi_sba::Client;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(Duration::from_secs(300))?;
    let resources = client.discover()?;
    let mut count = 0_u32;
    client.stream_loans(&resources.five_oh_four, |loan| {
        if loan.zip == "08840" {
            count += 1;
        }
    })?;
    println!("504 approvals with borrower ZIP 08840: {count}");
    Ok(())
}
