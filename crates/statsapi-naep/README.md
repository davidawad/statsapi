# statsapi-naep

`statsapi-naep` is a reusable blocking Rust client for querying and decoding the public NAEP Data Service API.

## Install

```toml
[dependencies]
statsapi-naep = { git = "https://github.com/davidawad/statsapi", package = "statsapi-naep" }
```

## Usage

This example is also available as [`examples/basic.rs`](examples/basic.rs) and is compiled by the crate build.

```rust
use statsapi_naep::{Client, Query};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let query = Query::new()
        .parameter("subject", "MAT")
        .parameter("grade", "4")
        .parameter("subscale", "MRPCM")
        .parameter("variable", "TOTAL")
        .parameter("jurisdiction", "CA")
        .parameter("stattype", "MN:MN")
        .parameter("Year", "Current");
    let response = Client::new().get(&query)?;
    println!("{}", serde_json::to_string_pretty(&response.result)?);
    Ok(())
}
```

## Authentication, endpoint, and cache

The documented NAEP Data Service endpoint is keyless; this client does not accept or read credentials. `Query` sends the documented API parameters to `https://www.nationsreportcard.gov/DataService/GetAdhocData.aspx` and `Response` preserves each JSON result row. `Client::with_cache_dir` enables an optional content-addressed response cache in a directory chosen by the caller. `Client::offline(true)` serves cached responses without making requests. The client retries HTTP 429, 5xx, and transport failures with bounded jittered delays.

Coverage is the endpoint's JSON `type=data` request and response format, including subject, grade, subscale, variable, jurisdiction, statistic type, and year parameters. It does not fetch separate bulk files or the API's comparison and significance request types.

The API documentation warns that results may change and says they are not intended to be stored in spreadsheets or databases. The optional cache is for caller-managed reuse, not an authoritative or permanent record; consider leaving it disabled when that is unsuitable.

## Terms

Reviewed 2026-11-22: [NAEP Data Service API documentation](https://www.nationsreportcard.gov/api_documentation.aspx), a U.S. federal source.

| Question | Finding from the upstream documentation |
| --- | --- |
| Automated access allowed? | The documentation explains direct machine-readable JSON queries and recommends splitting large requests into manageable calls. No prohibition on automated querying is stated. |
| Redistribution of data | The API page does not state a redistribution license. NAEP is U.S. federal public data; the API page warns against storing its changing results in spreadsheets or databases. |
| Attribution required? | No attribution requirement is stated on the API documentation page. Credit NAEP / NCES as the source as good practice. |
| Rate limits | No numeric rate limit is stated. The page warns about timeouts and recommends small requests. |

