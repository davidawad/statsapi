use statsapi_bls::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::from_env()?;
    let series = vec!["LNS14000000".to_owned()];
    for (series_id, observation) in client.laus_values(&series)? {
        match observation {
            Ok((value, vintage, source)) => {
                println!("{series_id}: {value} for {vintage} ({source})");
            }
            Err(error) => eprintln!("{series_id}: {error}"),
        }
    }
    Ok(())
}
