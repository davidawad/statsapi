use statsapi_irs_soi::{Client, Error};

fn main() -> Result<(), Error> {
    // IRS SOI datasets are public; no API key is required. Set a path to cache
    // downloads between runs, or use Client::new() for memory-only caching.
    let cache = std::env::var_os("IRS_SOI_DATA_CACHE_DIR").map(std::path::PathBuf::from);
    let mut client = cache.map_or_else(Client::new, Client::with_cache_dir);
    if let Some(returns) = client.zip_metric("10001", "returns")? {
        println!("ZIP 10001 reported {returns} returns");
    }
    Ok(())
}
