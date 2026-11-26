use statsapi_eviction_lab::latest_week;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let aggregate = latest_week(
        include_bytes!("../tests/fixtures/ets-weekly.csv"),
        "filings_avg_prepandemic_baseline",
        "12345",
    )?;

    println!("Eviction Lab synthetic weekly aggregate");
    println!("{:<14} {:>10} {:>12}", "Week", "Current", "Baseline");
    println!(
        "{:<14} {:>10.1} {:>12.1}",
        aggregate.week_date, aggregate.current, aggregate.comparison
    );
    Ok(())
}
