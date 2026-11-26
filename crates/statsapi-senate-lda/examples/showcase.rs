use statsapi_senate_lda::{parse_page, Summary};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let payload: serde_json::Value =
        serde_json::from_slice(include_bytes!("../tests/fixtures/filings-page.json"))?;
    let page = parse_page(&payload)?;
    let mut summary = Summary::default();
    summary.add(&page, "Q2");
    let (issue, _, count) = summary.top_issue().ok_or("missing issue")?;

    println!("Senate LDA synthetic sample");
    println!("{:<16} {:>10} {:>12}", "Metric", "Filings", "Value");
    println!("{:<16} {:>10} {:>12}", "Page total", page.count, "-");
    println!(
        "{:<16} {:>10} {:>12.2}",
        "Q2 spending", summary.filings, summary.spending
    );
    println!("{:<16} {:>10} {:>12}", "Top issue", count, issue);
    Ok(())
}
