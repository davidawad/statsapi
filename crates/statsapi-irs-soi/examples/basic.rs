use statsapi_irs_soi::{Client, Error};

fn main() -> Result<(), Error> {
    // IRS SOI datasets are public; no API key is required. The bulk file is
    // streamed and filtered, never stored; a path keeps the small per-ZIP extract
    // for 7 days as the fallback when the stream fails.
    let cache = std::env::var_os("IRS_SOI_DATA_CACHE_DIR").map(std::path::PathBuf::from);
    let mut client = cache.map_or_else(Client::new, Client::with_cache_dir);
    if let Some((returns, path)) = client.zip_metric("10001", "returns")? {
        println!("ZIP 10001 reported {returns} returns ({})", path.as_str());
    }
    Ok(())
}
