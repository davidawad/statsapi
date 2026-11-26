use statsapi_wikimedia_hiringlab::decode_hiring_lab_csv;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let country = decode_hiring_lab_csv(
        include_bytes!("../tests/fixtures/hiring-lab-country.csv"),
        "synthetic country fixture",
        "indeed_job_postings_index_SA",
        "jobcountry",
        "US",
    )?;
    let metro = decode_hiring_lab_csv(
        include_bytes!("../tests/fixtures/hiring-lab-metro.csv"),
        "synthetic metro fixture",
        "indeed_job_postings_index",
        "cbsa_code",
        "12345",
    )?;

    println!("Wikimedia Hiring Lab synthetic values");
    println!("Geography  Latest date  Latest  Prior");
    println!(
        "{:<9} {:<12} {:>6.1} {:>7.1}",
        "US", country.latest_date, country.latest, country.three_months_prior
    );
    println!(
        "{:<9} {:<12} {:>6.1} {:>7.1}",
        "12345", metro.latest_date, metro.latest, metro.three_months_prior
    );
    Ok(())
}
