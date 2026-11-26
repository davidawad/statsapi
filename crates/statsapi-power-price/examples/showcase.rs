use statsapi_power_price::UtilityRate;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut reader =
        csv::Reader::from_reader(include_bytes!("../tests/fixtures/utility-rates.csv").as_slice());
    let rate: UtilityRate = reader
        .deserialize()
        .next()
        .ok_or("no synthetic utility rate")??;

    println!("Power Price synthetic values");
    println!("{:<16} {:>12}", "Metric", "Value");
    println!("{:<16} {:>12}", "ZIP", rate.zip_code);
    println!("{:<16} {:>12.3}", "Residential", rate.res_rate);
    println!("{:<16} {:>12.3}", "Commercial", rate.comm_rate);
    println!("{:<16} {:>12.3}", "Industrial", rate.ind_rate);
    Ok(())
}
