# statsapi-statcan

`statsapi-statcan` is a reusable blocking Rust client for the Statistics Canada Web Data Service (WDS): latest values for any table coordinate, with geography resolved from cube metadata.

## Install

```toml
[dependencies]
statsapi-statcan = { git = "https://github.com/davidawad/statsapi", package = "statsapi-statcan" }
```

## Usage

This example is also available as [`examples/basic.rs`](examples/basic.rs):

```rust
use statsapi_statcan::{Client, CANADA_CODE};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let (geo, _, observation) =
        client.latest_for_geography(14_100_287, "{geo}.7.1.1.1.1", CANADA_CODE)?;
    println!("{}: {} ({})", geo.name, observation.value, observation.ref_period);
    Ok(())
}
```

## Authentication

None. The WDS is keyless.

## Endpoints covered

- `POST getCubeMetadata` (geography members and their SGC codes)
- `POST getDataFromCubePidCoordAndLatestNPeriods` (newest point for a coordinate)

Coordinates are padded to ten members; a `{geo}` placeholder in a template is replaced by the cube's member id for an SGC code (`11124` Canada, `35` Ontario, ...). The client retries 429, 5xx, transport errors and non-JSON maintenance pages with jittered backoff. It keeps no disk cache.

## Terms

| Topic | Statistics Canada terms |
| --- | --- |
| Automated access | Permitted: the WDS is a published machine API ([developers](https://www.statcan.gc.ca/en/developers/wds)) |
| Redistribution | Allowed under the [Statistics Canada Open Licence](https://www.statcan.gc.ca/en/reference/licence) |
| Attribution | Required: "Source: Statistics Canada, Table <id>, using Statistics Canada Open Licence" |
| Rate limits | 50 requests per second per server, 25 per second per IP |

Upstream terms: <https://www.statcan.gc.ca/en/terms-conditions>.
