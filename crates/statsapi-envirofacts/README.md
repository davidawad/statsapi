# statsapi-envirofacts

Blocking Rust client for EPA toxic-release (TRI) totals from Envirofacts and Superfund NPL sites near a point, with no key and nothing written to disk.

## Install

```toml
[dependencies]
statsapi-envirofacts = { git = "https://github.com/davidawad/statsapi", package = "statsapi-envirofacts" }
```

## Usage

See `examples/envirofacts.rs` (compiled by `cargo build --examples`):

```rust
use statsapi_envirofacts::{Client, TriArea};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let year = client.latest_tri_year(2026)?;
    let tri = client.tri_summary(&TriArea::County("34023".into()), year)?;
    println!("{year}: {} facilities, {} lbs", tri.facilities, tri.onsite_release_lbs);
    for site in client.npl_sites_near(40.5434, -74.3626, 25.0)?.iter().take(3) {
        println!("{} {:.1} km", site.name, site.distance_km);
    }
    Ok(())
}
```

## Auth and behavior

No key. Every call is a live request; the client keeps no cache. Transport errors, HTTP 429 and 5xx are retried three times with jittered backoff. Pages of 2,000 joined rows are read until a short page.

## Endpoints covered

- `https://data.epa.gov/efservice/tri_facility/{zip_code|state_county_fips_code}/<v>/tri_reporting_form/reporting_year/<y>/tri_release_qty/JSON`: one row per form and release medium; `total_release` (pounds) is summed across the on-site media (air, water, underground injection, landfill, land treatment, surface impoundment, other disposal). Dioxin forms report grams in the same field and are not converted.
- `https://data.epa.gov/efservice/tri_reporting_form/reporting_year/<y>/count/JSON` to find the latest reporting year.
- EPA's ArcGIS feature layer "Superfund National Priorities List (NPL) Sites with Status Information" (point-radius query).

## Terms

| Source | Automated access | Redistribution and attribution | Rate limits |
|---|---|---|---|
| [EPA Envirofacts](https://www.epa.gov/enviro/envirofacts-data-service-api) | Public REST service, no key. | US federal data, public domain; credit EPA TRI. | None stated; the client retries politely. |
| [EPA Superfund NPL](https://www.epa.gov/superfund/superfund-data-and-reports) | Public ArcGIS query service. | US federal data, public domain; credit EPA. | None stated. |

Fixtures are synthesized, not copied from upstream. Live tests are `#[ignore]`.
