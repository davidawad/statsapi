# statsapi-hmda

A small blocking Rust client downloads and decodes public HMDA application rows from the FFIEC Data Browser API.

## Install

```toml
[dependencies]
statsapi-hmda = { git = "https://github.com/davidawad/statsapi", package = "statsapi-hmda" }
```

## Usage

See [examples/basic.rs](examples/basic.rs) for the same compilable request:

```rust,no_run
use statsapi_hmda::{Client, CsvRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder().build()?;
    let request = CsvRequest::new(2025, "counties", "01001");
    let applications = client.applications(&request)?;
    println!("{} applications", applications.len());
    Ok(())
}
```

The API is public and keyless; no credentials or environment variables are needed. A cache is disabled by default. Call `.cache_dir(path)` to opt into a caller-owned compressed response cache, and `.offline()` to require a cached response. Each request retries HTTP 429, 5xx and transient connection or timeout errors with jittered backoff.

## Endpoints and data

The client calls `GET https://ffiec.cfpb.gov/v2/data-browser-api/view/csv` with `years` and a geography filter. `CsvRequest` supports the endpoint's `counties`, `states`, and `msamds` filters. `Application` decodes census tract, action taken, loan amount, derived race, derived ethnicity, HOEPA status, and loan purpose. The official API documentation describes the streamed CSV endpoint and its parameters at <https://ffiec.cfpb.gov/documentation/api/data-browser/>.

## Terms

Terms reviewed 2026-11-22 at the [FFIEC HMDA portal](https://ffiec.cfpb.gov/) and [Data Browser API documentation](https://ffiec.cfpb.gov/documentation/api/data-browser/).

| Question | Finding |
| --- | --- |
| Automated access allowed? | Yes. The official documentation describes the public API and CSV download endpoint. No restriction on automated access was stated in the reviewed materials. |
| Redistribution of data | The client does not redistribute data. HMDA public disclosures are published by U.S. federal agencies; no restriction on reuse of the public data was stated in the reviewed materials. Respect HMDA's privacy-modified public disclosure rules. |
| Attribution required? | No attribution requirement was stated in the reviewed API materials. Citing FFIEC/CFPB as the data source is appropriate. |
| Rate limits | No numeric request limit was stated in the reviewed materials. Use the optional cache and avoid unnecessary requests. |
