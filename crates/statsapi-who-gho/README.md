# statsapi-who-gho

`statsapi-who-gho` is a reusable blocking Rust client for reading WHO Global Health Observatory OData indicators.

## Install

```toml
[dependencies]
statsapi-who-gho = { git = "https://github.com/davidawad/statsapi", package = "statsapi-who-gho" }
```

## Usage

The same request-building flow is in [`examples/basic.rs`](examples/basic.rs):

```rust
use statsapi_who_gho::{latest_observation, IndicatorQuery, WhoGhoClient};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let query = IndicatorQuery::new("WHOSIS_000002")?
        .with_dimension("Dim1", "SEX_BTSX")?;
    let client = WhoGhoClient::new()?.with_cache_dir("./who-cache");
    let (url, payload) = client.fetch(&query, "USA")?;
    println!("{url}");
    if let Some(row) = latest_observation(&payload) {
        println!("{} ({})", row["NumericValue"], row["TimeDim"]);
    }
    Ok(())
}
```

WHO GHO is public and requires no API key. No credential or key environment
variable is read. Pass an optional cache directory with `with_cache_dir`; call
`offline()` to require cached data. Retries use jitter for HTTP 429, HTTP 5xx,
and transport errors. Errors retain their underlying HTTP, JSON, or I/O cause.

## Coverage

The client reads the WHO GHO OData endpoint at
`https://ghoapi.azureedge.net/api/{IndicatorCode}`, builds filters for
`SpatialDim` and `Dim1` through `Dim6`, decodes OData JSON, and selects the
newest non-null numeric observation. Country codes must be uppercase ISO
3166-1 alpha-3. API responses may identify third-party source material; check
the indicator metadata before reusing a particular series.

## Terms

Checked 2026-11-22 against [WHO Data Policy](https://www.who.int/about/policies/publishing/data-policy)
and the [terms and conditions for WHO data compilations](https://www.who.int/about/policies/publishing/data-policy/terms-and-conditions).
The policy page links the terms governing access and reuse of published WHO
datasets; it does not set a numeric API rate limit or expressly discuss
automated clients.

| Question | Finding |
| --- | --- |
| Automated access allowed? | The policy does not expressly address automation. The public OData API is available without login; no prohibition on automated reads is stated in the cited policy. |
| Redistribution of data | WHO terms grant rights to use, reproduce, extract, download, copy, and distribute data for public health purposes. Redistribution is constrained by that purpose and does not cover third-party credited material. |
| Attribution required? | Yes. Attribute WHO and the contributing country or countries using dataset title, year, access date, and country acknowledgement; verify metadata for each series. |
| Rate limits | No numeric limit is stated on the policy page. Use the client’s retry behavior and avoid abusive request rates. |

The client is provided as software and does not bundle upstream data. WHO terms
may change and series may carry additional third-party restrictions.

## Verification

DECIDED: expose query construction, HTTP access, retries, cache handling, and
OData decoding from one no-repository-dependency crate -- this keeps the source
adapter on the reusable implementation while leaving place resolution and
adapter records in `who-gho-data`.

Verified 2026-11-22 in the Rust workspace: `cargo fmt -p statsapi-who-gho -p
who-gho-data --check` passed (2 crates); `cargo clippy --no-deps -p
statsapi-who-gho -p who-gho-data --all-targets -- -D warnings` passed (2
crates); `cargo test -p statsapi-who-gho -p who-gho-data` passed (9 passed, 2
ignored); `cargo test -p statsapi-who-gho -- --ignored` passed (1 live test);
`cargo build -p statsapi-who-gho --example basic` passed; standalone workspace
check passed. Offline fixtures use synthetic values only; no upstream values
were copied into fixtures.
