# statsapi-ucdp-history

`statsapi-ucdp-history` is a small Rust client for downloading and decoding
historical UCDP/PRIO conflict CSV releases. It returns rows keyed by the
published column names and never bundles or persists upstream data.

## Install

```toml
[dependencies]
statsapi-ucdp-history = { git = "https://github.com/davidawad/statsapi", package = "statsapi-ucdp-history" }
```

## Usage

```rust,no_run
use statsapi_ucdp_history::{decode_csv, fetch_archive_member};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let csv = fetch_archive_member(
        "https://ucdp.uu.se/downloads/ucdpprio/ucdp-prio-acd-251.zip",
        "ucdp-prio-acd.csv",
    )?;
    let rows = decode_csv(&csv)?;
    println!("{} rows", rows.len());
    Ok(())
}
```

The client has no authentication mechanism: UCDP downloads are public, and the
caller supplies the release URL and archive member. Downloads and extracted
members are limited to 16 MiB and are held in memory only.

## Data terms

| Upstream data | Access and terms | Attribution |
| --- | --- | --- |
| UCDP/PRIO and related UCDP datasets | Free downloads under CC BY 4.0; redistribution is allowed with the required citation. | Cite the relevant UCDP dataset and publications listed by UCDP. |

See the [UCDP dataset download center](https://ucdp.uu.se/downloads/) and
[UCDP API documentation](https://ucdp.uu.se/apidocs/).

## Verified

- Tests use a synthesized CSV fixture; no upstream data is bundled.
- The decoder does not write downloaded data to disk.
