# statsapi-nara

`statsapi-nara` is a blocking client for the National Archives Catalog API v2.

## Install

```toml
[dependencies]
statsapi-nara = { git = "https://github.com/davidawad/statsapi", package = "statsapi-nara" }
```

## Usage

The same flow is compiled from [`examples/basic.rs`](examples/basic.rs). NARA requires a caller-owned API key in `NARA_API_KEY` before the example is run.

```rust,no_run
use std::env;
use statsapi_nara::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let response = Client::new(env::var("NARA_API_KEY")?)?.search("constitution", 10)?;
    println!("{} response fields", response.as_object().map_or(0, |v| v.len()));
    Ok(())
}
```

An API key is required on every request. The example reads it from the environment and never stores a key in source.

## Coverage

| Upstream endpoint or file | Supported access |
| --- | --- |
| NARA Catalog API v2 records/search | Keyword search, result limit, API-key header, and JSON response decoding |
| Catalog JSON | Synthetic or cached response decoding for offline callers |

## NARA terms

Terms reviewed 2026-11-26: [NARA Catalog API terms and access](https://www.archives.gov/research/catalog/help/api).

| Question | Terms summary |
| --- | --- |
| Automated access allowed? | Yes. NARA offers machine-readable public Catalog data, but users must not attack, manipulate, or scrape the service. |
| Redistribution of data | Catalog metadata may be incorporated into applications and datasets; do not use the API to download the entire Catalog, use the bulk dataset instead. |
| Attribution required? | Preserve NARA source context and follow the Catalog and Archives.gov website policies; do not imply endorsement. |
| Rate limits | The default limit is 10,000 queries per month per API key; exceeding it temporarily blocks the key until the next month. |
