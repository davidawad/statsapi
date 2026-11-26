use statsapi_fbi_cde::{year_sum, Client, Offense};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::from_env()?;
    let summary = client.state_summary("NJ", Offense::Violent, 2022, 2023)?;
    if let Some((name, monthly)) = summary.series_other_than(&[]) {
        println!(
            "{name} 2023 violent crime per 100k: {:?}",
            year_sum(monthly, 2023)
        );
    }
    println!("{} agencies in NJ", client.agencies_by_state("NJ")?.len());
    Ok(())
}
