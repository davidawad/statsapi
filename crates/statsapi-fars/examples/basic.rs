use statsapi_fars::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Pass an optional cache path explicitly; omit it for an uncached request.
    let client = Client::new().with_cache_dir("./fars-cache");
    let data = client.fetch_year(2024)?;
    println!("{} state rows", data.states().len());
    Ok(())
}
