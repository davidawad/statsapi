# statsapi-usda-food-access

`statsapi-usda-food-access` is a reusable Rust client for downloading and decoding USDA ERS food access and FNS SNAP data files.

## Install

```toml
[dependencies]
statsapi-usda-food-access = { git = "https://github.com/davidawad/statsapi", package = "statsapi-usda-food-access" }
```

## Usage

The same call is compiled from [`examples/basic.rs`](examples/basic.rs):

```rust,no_run
use statsapi_usda_food_access::Client;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache = std::env::var_os("USDA_FOOD_ACCESS_CACHE_DIR").map(PathBuf::from);
    let client = Client::new(cache)?;
    let observation = client.atlas_value("01001020100", "lapop1")?;
    println!("{} ({})", observation.value, observation.vintage);
    Ok(())
}
```

Pass an optional cache directory to `Client::new`; the client never chooses a global cache path. Set `USDA_FOOD_ACCESS_OFFLINE=1` to use cached files only. These public downloads do not require an API key or other credentials. The client uses blocking `reqwest` with rustls and retries transport errors, HTTP 429, and server errors with bounded jitter.

## Authentication

No authentication is needed for the covered USDA ERS and FNS downloads. The client accepts no key and sends no credentials.

## Coverage

| Upstream file | Supported decoding |
| --- | --- |
| [ERS 2019 Large Retailer Access Map (LRAM) archive](https://www.ers.usda.gov/data-products/food-access-research-atlas/download-the-data) | Tract values from `Food Access Research Atlas.csv` |
| [FNS SNAP project-area archive](https://www.fns.usda.gov/pd/supplemental-nutrition-assistance-program-snap) | County totals from the July 2025 workbook |
| FNS monthly persons and households workbooks | Latest published state values and workbook vintage |

The client currently exposes `atlas_value`, `county_value`, and `state_value`. It does not interpret place schemas or indicators. An application can provide its own cache directory and map these typed observations into its own data model.

## USDA terms

Terms read 2026-11-22. This is a summary of the linked U.S. federal source pages, not a replacement for their terms or notices.

| Question | ERS and FNS source terms summary |
| --- | --- |
| Automated access allowed? | The cited pages offer downloadable data files for public use and do not state a prohibition on automated retrieval. No API key is required for the covered files. |
| Redistribution of data | These are USDA federal data products. The download pages make the files available for use; the client redistributes no upstream data values. Review notices attached to an individual file before redistributing that file. |
| Attribution required? | The cited download pages do not state an attribution condition. Cite USDA ERS or USDA FNS and the file vintage as good source practice. Do not imply USDA endorsement. |
| Rate limits | The cited download pages publish no numeric rate limit. Use the downloads responsibly; the client retries only transient transport, 429, and server failures with bounded backoff. |

Sources: [USDA Economic Research Service](https://www.ers.usda.gov/), [ERS Food Access Research Atlas downloads](https://www.ers.usda.gov/data-products/food-access-research-atlas/download-the-data), [USDA Food and Nutrition Service](https://www.fns.usda.gov/), and [FNS SNAP data page](https://www.fns.usda.gov/pd/supplemental-nutrition-assistance-program-snap). Both agencies are U.S. federal agencies.

