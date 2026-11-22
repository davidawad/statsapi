# statsapi-bea

`statsapi-bea` gives Rust applications a reusable blocking client for retrieving and decoding BEA Regional API data.

## Install

```toml
[dependencies]
statsapi-bea = { git = "https://github.com/davidawad/statsapi", package = "statsapi-bea" }
```

## Usage

Set `BEA_API_KEY`, or pass a key directly with `Client::new`. An optional cache directory is supplied by the caller; the client keeps no global cache state.

```rust,no_run
use statsapi_bea::{Client, Result};

fn main() -> Result<()> {
    let client = Client::from_env()?.with_cache_dir("./bea-cache");
    let observation = client.get_data("CAINC1", "3", "00000", "LAST5")?;
    println!("{}: {}", observation.year, observation.value);
    Ok(())
}
```

The same example is available at [`examples/basic.rs`](examples/basic.rs) and is built by the crate's example target.

## Authentication and coverage

The client reads `BEA_API_KEY` with `Client::from_env`, or accepts a credential directly through `Client::new`. It never includes the key in `Observation::source_url`. It currently covers the BEA Regional API `GetData` endpoint (`https://apps.bea.gov/api/data/`), including Regional tables and latest matching line decoding. No bulk-file endpoint is covered.

| Terms question | Finding |
|---|---|
| Automated access allowed? | Yes. The API terms allow services to retrieve, display, and analyze BEA data. |
| Redistribution of data | The API terms do not state a general redistribution permission or prohibition. Check applicable data-specific terms before redistributing. |
| Attribution required? | Yes. Services using the API should prominently display: “This product uses the Bureau of Economic Analysis (BEA) Data API but is not endorsed or certified by BEA.” |
| Rate limits | BEA may impose access, call, or use limits and may block attempts to exceed or circumvent them; no numeric limit is stated in the terms. |

Source: [BEA API Terms of Service](https://apps.bea.gov/API/_pdf/bea_api_tos.pdf), read 2026-11-22. API use is permitted for retrieval and analysis, so this client may be used for its intended purpose; the terms' attribution and usage limits still apply.
