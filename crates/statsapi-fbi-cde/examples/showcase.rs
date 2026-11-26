use statsapi_fbi_cde::{year_mean, Summary};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let summary = Summary::from_slice(include_bytes!("../tests/fixtures/agency-summary.json"))?;
    let (series, monthly) = summary
        .series_other_than(&[])
        .ok_or("FBI CDE fixture has no synthetic rate series")?;
    let mean = year_mean(monthly, 2023).ok_or("FBI CDE fixture has no 2023 values")?;

    println!("FBI Crime Data Explorer synthetic values");
    println!("{:<32} {:>8} {:>12}", "Series", "Year", "Mean rate");
    println!("{series:<32} {:>8} {:>12.1}", 2023, mean);
    Ok(())
}
