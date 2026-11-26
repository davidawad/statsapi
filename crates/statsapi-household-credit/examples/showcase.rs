use statsapi_household_credit::{
    county_debt_to_income_bound, ny_state_value, urban_value, CountyBound, NyMetric,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let urban = urban_value(
        include_bytes!("../tests/fixtures/urban_middlesex.csv"),
        "county",
        "99999",
        "totcoll",
    )?
    .ok_or("household fixture has no synthetic Urban value")?;
    let ny = ny_state_value(
        include_bytes!("../tests/fixtures/ny_hhdc.xlsx"),
        "ZZ",
        NyMetric::StateDebtPerCapita,
    )?
    .ok_or("household fixture has no synthetic NY Fed value")?;
    let county = county_debt_to_income_bound(
        include_bytes!("../tests/fixtures/fed_county_interval.zip"),
        "99999",
        CountyBound::Low,
    )?
    .ok_or("household fixture has no synthetic county value")?;

    println!("Household credit datasets synthetic values");
    println!("{:<18} {:>12} {:<24}", "Dataset", "Value", "Vintage");
    println!("{:<18} {:>12.2} {:<24}", "Urban", urban.0, urban.1);
    println!("{:<18} {:>12.1} {:<24}", "NY Fed", ny.0, ny.1);
    println!(
        "{:<18} {:>12.2} {:<24}",
        "Fed county low", county.0, county.1
    );
    Ok(())
}
