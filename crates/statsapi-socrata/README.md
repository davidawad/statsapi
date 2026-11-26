# statsapi-socrata

`statsapi-socrata` builds Socrata Open Data API queries, retrieves JSON rows, and optionally caches responses for Rust applications.

## Install

```toml
[dependencies]
statsapi-socrata = { git = "https://github.com/davidawad/statsapi", package = "statsapi-socrata" }
```

## Usage

The same runnable example is in [`examples/basic.rs`](examples/basic.rs).

```rust,no_run
use statsapi_socrata::SocrataClient;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = SocrataClient::builder();
    if let Ok(token) = std::env::var("SOCRATA_APP_TOKEN") {
        if !token.trim().is_empty() {
            builder = builder.app_token(token);
        }
    }
    let client = builder.cache_dir("./socrata-cache").build()?;
    let url = SocrataClient::resource_url(
        "data.cdc.gov",
        "cwsq-ngmh",
        &[("limit".to_string(), "1".to_string())],
    )?;
    let rows = client.get_rows(&url)?;
    println!("{} row(s)", rows.len());
    Ok(())
}
```

`SOCRATA_APP_TOKEN` is optional for public read queries; the caller reads it and passes the value to the client. Cache storage is opt-in and its directory is supplied by the caller. `ClientBuilder::offline()` restricts reads to cached responses.

## Authentication

Public read queries may be anonymous. An application token is passed with `SocrataClient::builder().app_token(token)`; this crate does not read process environment or credential stores. Socrata documents tokens as a way to attribute and throttle requests per application, and recommends them for higher throttling limits.

## Endpoints and files covered

DECIDED: use SODA resource JSON GET endpoints -- this matches the existing CDC adapter traffic and supports the datasets already in scope.

This client builds SODA 2.x resource endpoints (`https://{domain}/resource/{dataset}.json`), encodes SoQL query parameters, sends blocking HTTPS GET requests using rustls, retries 429, 5xx, and transport failures with bounded jitter, and decodes JSON arrays of records. It supports any Socrata domain and dataset ID supplied by the caller. This release covers resource JSON responses; it does not download bulk export files or implement write APIs.

## Terms reviewed 2026-11-22

| Question | Finding for this client | Source |
| --- | --- | --- |
| Automated access allowed? | Yes. Socrata documents its Open Data API and anonymous read queries. Keep requests deliberate; abusive use may be throttled. | [Socrata Developers](https://dev.socrata.com/), [application tokens and throttling](https://dev.socrata.com/docs/app-tokens) |
| Redistribution of data? | Depends on the publisher and dataset license. CDC's public API catalog identifies the CDC Open Data API as CC0; CDC says most agency materials are public domain, with exceptions. Check each dataset before redistribution. | [CDC API catalog license](https://catalog.data.gov/dataset/cdc-open-data-api), [CDC use of agency materials](https://www.cdc.gov/other/agencymaterials.html) |
| Attribution required? | No general attribution requirement was identified for CDC public-domain material; attribution may be required by an individual publisher or dataset license. This client preserves source URLs so callers can cite datasets. | [CDC use of agency materials](https://www.cdc.gov/other/agencymaterials.html), dataset-specific terms |
| Rate limits? | Yes. Anonymous traffic is subject to shared-IP throttling; 429 responses are retried with bounded jitter. Limits may change, and abusive or malicious usage may be throttled. | [Socrata application tokens and throttling](https://dev.socrata.com/docs/app-tokens) |

The CDC portal is a U.S. federal portal. Its [portal catalog](https://data.cdc.gov/) links dataset-level metadata; check each dataset's `license` field because CDC's public-domain status does not automatically apply to every dataset. Terms reviewed 2026-11-22; confirm the dataset's current license and portal terms before reuse.

