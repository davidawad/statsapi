# statsapi-fema

`statsapi-fema` provides blocking access to OpenFEMA and FEMA ArcGIS JSON endpoints.

## Install

```toml
[dependencies]
statsapi-fema = { git = "https://github.com/davidawad/statsapi", package = "statsapi-fema" }
```

## Usage

```rust
use statsapi_fema::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let response = Client::new().get_json(
        "https://www.fema.gov/api/open/v3/NfipClaims?$top=1",
    )?;
    println!("{}", response);
    Ok(())
}
```

`Client::with_cache_dir` opts into caller-owned content-addressed response caching. `offline(true)` requires a cache hit. The client retries transient transport failures and HTTP 429/5xx responses. It covers OpenFEMA JSON collections and public FEMA-hosted ArcGIS REST JSON.

## Terms

| Source | Automated access | Redistribution and attribution | Rate limits |
|---|---|---|---|
| [OpenFEMA](https://www.fema.gov/about/openfema/data-sets) | Public REST API; requests should be paginated. | Federal data is generally public domain; retain FEMA source attribution. | No numeric limit stated in the dataset portal. |
| [FEMA National Risk Index](https://hazards.fema.gov/nri/data-resources) | Public ArcGIS query services. | Public federal data; credit FEMA. | No numeric limit stated. |

Responses are not copied into fixtures; tests use synthesized values.
