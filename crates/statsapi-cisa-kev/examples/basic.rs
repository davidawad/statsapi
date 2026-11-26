//! Prints the KEV additions of the last 30 days and the NVD critical count.
use chrono::Utc;
use statsapi_cisa_kev::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let now = Utc::now().naive_utc();
    let summary = client.fetch_catalog()?.summarize(now.date())?;
    println!(
        "KEV: {} added in 30 days of {}",
        summary.added_30d, summary.catalog_size
    );
    let (critical, _) = client.critical_cves_published(now, 7)?;
    println!("NVD critical CVEs published in 7 days: {critical}");
    Ok(())
}
