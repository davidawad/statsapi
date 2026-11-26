use statsapi_cdc_wonder::{latest_row, parse_wonder_table};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rows = parse_wonder_table(include_str!("../tests/fixtures/ucd-suicide.xml"))?;
    let row = latest_row(&rows).ok_or("CDC WONDER fixture has no rate")?;

    println!("CDC WONDER synthetic sample");
    println!("{:<12} {:>12} {:>12}", "Year", "Deaths", "Rate");
    println!(
        "{:<12} {:>12} {:>12.2}",
        row.year,
        row.deaths.ok_or("CDC fixture has no deaths")?,
        row.age_adjusted_rate.ok_or("CDC fixture has no rate")?
    );
    Ok(())
}
