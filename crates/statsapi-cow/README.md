# statsapi-cow

`statsapi-cow` is a small Rust client for downloading and decoding public
Correlates of War (COW) CSV and ZIP releases. It returns rows keyed by the
published column names and never bundles or persists upstream data.

## Install

```toml
[dependencies]
statsapi-cow = { git = "https://github.com/davidawad/statsapi", package = "statsapi-cow" }
```

## Usage

```rust,no_run
use statsapi_cow::{decode_csv, fetch_archive_member};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let csv = fetch_archive_member(
        "https://correlatesofwar.org/wp-content/uploads/COW-country-codes.zip",
        "COW-country-codes.csv",
    )?;
    let rows = decode_csv(&csv)?;
    println!("{} rows", rows.len());
    Ok(())
}
```

The client has no authentication mechanism: COW downloads are public, and the
caller supplies the release URL and archive member. Downloads and extracted
members are limited to 16 MiB and are held in memory only.

## Data terms

| Upstream data | Access and terms | Attribution |
| --- | --- | --- |
| Correlates of War Project datasets | Public downloads; users agree to the COW download terms, including the dataset-specific citation and redistribution restrictions. | Cite each dataset as specified on its COW page. |

See the [COW data sets and terms](https://correlatesofwar.org/data-sets/).

