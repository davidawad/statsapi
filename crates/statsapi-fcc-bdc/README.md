# statsapi-fcc-bdc

`statsapi-fcc-bdc` is a blocking Rust client for FCC Broadband Data Collection availability releases.

## Install

```toml
[dependencies]
statsapi-fcc-bdc = { git = "https://github.com/davidawad/statsapi", package = "statsapi-fcc-bdc" }
```

## Usage

The same flow is compiled from [`examples/basic.rs`](examples/basic.rs). Set `FCC_BDC_USERNAME` and `FCC_BDC_HASH_VALUE` before running it.

```rust,no_run
use std::env;
use statsapi_fcc_bdc::{Client, Credentials};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let credentials = Credentials::new(
        env::var("FCC_BDC_USERNAME")?,
        env::var("FCC_BDC_HASH_VALUE")?,
    ).ok_or("FCC BDC credentials are required")?;
    let availability = Client::new(credentials)?.county("06037")?;
    println!("{} units", availability.total_units);
    Ok(())
}
```

FCC BDC requires a caller-supplied username and `hash_value` token. The example reads both from the environment and never hardcodes a key.

## Coverage

| Upstream endpoint or file | Supported access |
| --- | --- |
| Public map release listing | Latest availability release discovery |
| County availability ZIP/CSV | Total units and 100/20 Mbps share for a county |
| Provider summary ZIP/CSV | Count of residential providers reporting coverage |

## FCC BDC terms

Terms reviewed 2026-11-26. See the [FCC BDC public map documentation](https://bdc.fcc.gov/public-map/) and [FCC website terms](https://www.fcc.gov/terms) for the controlling conditions.

| Question | Terms summary |
| --- | --- |
| Automated access allowed? | The documented public map API supports programmatic access with registered BDC credentials; callers must follow FCC service conditions. |
| Redistribution of data | Availability files are published government data, but downstream users must preserve any FCC notices and check the current release terms before redistribution. |
| Attribution required? | Identify the FCC Broadband Data Collection as the source and retain source-release metadata. |
| Rate limits | The documentation does not promise an unlimited request rate; callers should use a bounded, respectful request schedule and honor service responses. |
