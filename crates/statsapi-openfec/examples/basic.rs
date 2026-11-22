use statsapi_openfec::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::from_env()?;
    let zip_aggregates = client.schedule_a_by_zip("00000", 2024)?;
    let state_aggregates = client.schedule_a_by_state("XX", 2024)?;
    println!(
        "ZIP candidate contributions: {} dollars across {} committees; state: {} dollars",
        zip_aggregates.total,
        zip_aggregates.committees.len(),
        state_aggregates.total
    );
    Ok(())
}
