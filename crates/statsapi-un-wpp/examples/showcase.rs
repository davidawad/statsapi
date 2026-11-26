use statsapi_un_wpp::parse_csv;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let france = parse_csv(
        include_bytes!("../tests/fixtures/wpp-medium-sample.csv").as_slice(),
        "FRA",
    )?
    .ok_or("missing synthetic country")?;
    let (year, population) = france
        .latest_value("TPopulation1July", 2050)
        .ok_or("missing value")?;

    println!("UN World Population Prospects synthetic sample");
    println!("{:<16} {:<12} {:>14}", "Country", "Year", "Population");
    println!("{:<16} {:<12} {:>14.2}", france.location, year, population);
    println!("{:<16} {:<12} {:>14}", "Rows", france.rows.len(), "-");
    Ok(())
}
