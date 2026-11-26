use statsapi_ioda::{compare, parse_episodes, parse_signal};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let episodes = parse_episodes(include_str!("../tests/fixtures/events.json"))?;
    let signal = parse_signal(include_str!("../tests/fixtures/signal.json"))?;
    let comparison = compare(&signal).ok_or("missing signal comparison")?;

    println!("IODA synthetic sample");
    println!("{:<16} {:>10} {:>10}", "Measure", "Value", "Unit");
    println!("{:<16} {:>10} {:>10}", "Episodes", episodes.len(), "count");
    println!(
        "{:<16} {:>10.1} {:>10}",
        "Current", comparison.current, "signal"
    );
    println!(
        "{:<16} {:>10.1} {:>10}",
        "Baseline", comparison.baseline, "signal"
    );
    Ok(())
}
