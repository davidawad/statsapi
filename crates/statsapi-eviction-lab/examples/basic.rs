use statsapi_eviction_lab::{Client, NetworkPolicy};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(None)?;
    let url = "https://dev.evictionlab.org/uploads/philadelphia_weekly_2020_2021.csv";
    let csv = client.fetch(url, NetworkPolicy::Allow)?;
    let latest = client.latest_week(&csv, "filings_avg", "")?;
    println!(
        "week ending {}: current aggregate {}",
        latest.week_date, latest.current
    );
    Ok(())
}
