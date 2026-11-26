# statsapi-noaa

`statsapi-noaa` downloads and parses NOAA NCEI Storm Events county detail files by year.

## Install

```toml
[dependencies]
statsapi-noaa = { git = "https://github.com/davidawad/statsapi", package = "statsapi-noaa" }
```

## Usage

```rust
use statsapi_noaa::Client;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rows = Client::new().details(2024)?;
    println!("{} storm events", rows.len());
    Ok(())
}
```

Files are discovered from NOAA's annual bulk-file index, decompressed and normalized to event year, state and county FIPS, event type, and property/crop damage. The caller can set an optional cache directory or use `offline(true)`.

## Terms

| Source | Automated access | Redistribution and attribution | Rate limits |
|---|---|---|---|
| [NOAA NCEI Storm Events](https://www.ncei.noaa.gov/pub/data/swdi/stormevents/csvfiles/) | Public bulk CSV/GZIP files are intended for download. | Federal public data; credit NOAA NCEI. | No numeric limit stated; bulk yearly files are cached once per URL. |

Fixtures are synthesized and retain only the upstream field layout.
