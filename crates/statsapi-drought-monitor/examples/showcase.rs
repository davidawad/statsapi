use statsapi_drought_monitor::{parse_weeks, summarize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let weeks = parse_weeks(include_str!("../tests/fixtures/statistics.json"))?;
    let summary = summarize(&weeks).ok_or("synthetic fixture has no weeks")?;

    println!("US Drought Monitor synthetic statistics");
    println!("{:<12} {:>8} {:>8}", "Week", "D2-D4", "D4");
    println!(
        "{:<12} {:>8.1} {:>8.1}",
        summary.latest.map_date, summary.latest.d2, summary.latest.d4
    );
    Ok(())
}
