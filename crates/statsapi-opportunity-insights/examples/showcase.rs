use statsapi_opportunity_insights::CsvTable;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let table = CsvTable::from_bytes(include_bytes!("../tests/fixtures/social_capital_zip.csv"))?;
    let row = table.rows().first().ok_or("no synthetic row")?;

    println!("Opportunity Insights synthetic values");
    println!("{:<16} {:>12}", "Metric", "Value");
    for (label, names) in [
        ("ZIP", &["zip"][..]),
        ("EC score", &["ec_zip"][..]),
        ("Clustering", &["clustering_zip"][..]),
    ] {
        println!(
            "{label:<16} {:>12}",
            table.cell(row, names).ok_or("missing synthetic cell")?
        );
    }
    Ok(())
}
