# statsapi-imf-datamapper

`statsapi-imf-datamapper` is a reusable blocking Rust client for the IMF DataMapper API, which serves the World Economic Outlook (WEO) forecast series. It has no repo dependencies.

## Usage

This example is also available as [`examples/basic.rs`](examples/basic.rs):

```rust
use statsapi_imf_datamapper::{horizon, Client};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    if let Some(series) = client.country_series("NGDP_RPCH", "USA")? {
        if let Some(latest) = horizon(&series, 2026, 0) {
            println!("{}: {}", latest.year, latest.value);
        }
    }
    Ok(())
}
```

## Authentication

None. `https://www.imf.org/external/datamapper/api/v1/{indicator}` is keyless JSON.

## Endpoints and data

- `GET /{indicator}` returns `{"values": {indicator: {ISO3: {year: value}}}}` for every country. The API ignores a country path segment (`/{indicator}/USA` returns the same all-country body), so the client filters locally. About 120 KB per indicator.
- Indicators used by `imf-weo-data`: `NGDP_RPCH` (real GDP growth, %), `PCPIPCH` (inflation, average consumer prices, %), `LUR` (unemployment rate, %), `BCA_NGDPD` (current account, % of GDP), `GGXWDG_NGDP` (gross government debt, % of GDP).
- The body does not mark which years are estimates, so `horizon` treats years before the supplied current year as actual and the current year onward as projection.
- Quirk: the IMF edge (Akamai) returns 403 for every `User-Agent` except a curl one, including an empty one. The client sends `curl/8.5.0`.
- Nothing is cached or written to disk. Requests retry transport errors, 429 and 5xx with bounded exponential delay.

## Terms

Terms reviewed 2026-11-23 at [IMF Copyright and Usage](https://www.imf.org/en/About/copyright-and-terms).

| Question | Terms summary |
| --- | --- |
| Automated access allowed? | The DataMapper API is published openly with no key and no stated prohibition on programmatic access; keep volume low (one request per indicator per run). |
| Redistribution of data | IMF content may be used and reproduced for non-commercial purposes with attribution; commercial reuse needs permission. This crate distributes no upstream data. |
| Attribution required? | Yes: cite the IMF World Economic Outlook database as the source. |
| Rate limits | None published; the edge may block abusive volume. |

