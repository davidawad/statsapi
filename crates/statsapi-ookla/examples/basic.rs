use std::env;
use std::path::PathBuf;

use statsapi_ookla::{aggregate_reader, download, prefixes, value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = env::var("OOKLA_PARQUET_URL")?;
    let path = env::var_os("OOKLA_PARQUET_PATH").map_or_else(
        || env::temp_dir().join("statsapi-ookla-example.parquet"),
        PathBuf::from,
    );
    let response = download(&url)?;
    let totals = aggregate_reader(response, &prefixes(34.05, -118.25), &path)?;
    println!("download Mbps: {:?}", value(&totals, "download_mbps"));
    std::fs::remove_file(path)?;
    Ok(())
}
