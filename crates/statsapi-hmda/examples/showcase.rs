use statsapi_hmda::decode_csv;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let applications = decode_csv(include_bytes!("../tests/fixtures/applications.csv"))?;

    println!("HMDA mortgage applications synthetic values");
    println!(
        "{:<16} {:>8} {:>12}",
        "Census tract", "Action", "Loan amount"
    );
    for application in applications.iter().take(4) {
        println!(
            "{:<16} {:>8} {:>12}",
            application.census_tract,
            application.action_taken,
            application
                .loan_amount
                .map_or_else(|| "missing".to_owned(), |amount| format!("{amount:.0}"))
        );
    }
    Ok(())
}
