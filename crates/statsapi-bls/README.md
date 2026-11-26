# statsapi-bls

`statsapi-bls` is a reusable Rust client for retrieving and decoding BLS labor statistics.

## Install

```toml
[dependencies]
statsapi-bls = { git = "https://github.com/davidawad/statsapi", package = "statsapi-bls" }
```

## Usage

The same flow is compiled from [`examples/basic.rs`](examples/basic.rs):

```rust,no_run
use statsapi_bls::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::from_env()?;
    let rows = client.laus_values(&["LNS14000000".to_owned()])?;
    for (id, result) in rows {
        if let Ok((value, period, source)) = result {
            println!("{id}: {value} ({period}) from {source}");
        }
    }
    Ok(())
}
```

`Client::new(Some(api_key), Some(cache_dir))` accepts a caller-supplied key and optional cache directory. `Client::from_env()` reads `BLS_API_KEY`; it does not select a cache directory. Set `BLS_DATA_OFFLINE=1` to read from the configured cache only. Credentials are never logged.

## Coverage

| Upstream endpoint or file | Supported access |
| --- | --- |
| BLS Public Data API v2 time series | Batched LAUS series requests and CES series requests; monthly observation decoding |
| QCEW Open Data area CSV | County area-file retrieval and average weekly wage decoding |

The client constructs LAUS county, CBSA, state, and national identifiers. CES employment supports state and CBSA identifiers. QCEW currently requests the 2025 Q4 county CSV slice. Calls use blocking `reqwest` with rustls, a 90-second request timeout, an optional caller-owned SHA-256 cache, and bounded jittered retries for transport errors, HTTP 429 and server errors.

## BLS terms

Terms read 2026-11-22. This is a summary of the linked BLS pages, not a replacement for them.

| Question | BLS terms summary |
| --- | --- |
| Automated access allowed? | The public API is documented for users; the terms do not prohibit automated access. BLS may limit calls or block users who exceed or circumvent limits. |
| Redistribution of data | BLS says its published material is public domain, excluding previously copyrighted photos and illustrations. The API terms say downloaded data have no end-use controls. |
| Attribution required? | Cite BLS as the source and include the date data were accessed. State that BLS cannot vouch for post-retrieval data or analyses. Do not use the BLS logo. |
| Rate limits | BLS may set access/call limits; the terms provide no fixed numeric rate. Respect API responses and do not evade limits. |

Sources: [BLS Copyright Information](https://www.bls.gov/opub/copyright-information.htm) and [BLS Developers Terms of Service](https://www.bls.gov/developers/termsOfService.htm).

