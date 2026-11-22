# statsapi-county-health-rankings

A small blocking Rust client fetches and decodes County Health Rankings & Roadmaps annual analytic CSV data.

## Install

```toml
[dependencies]
statsapi-county-health-rankings = { git = "https://github.com/davidawad/statsapi", package = "statsapi-county-health-rankings" }
```

## Usage

See [`examples/basic.rs`](examples/basic.rs): it fetches the published annual CSV and decodes a county measure.

```rust,no_run
use statsapi_county_health_rankings::Client;

let client = Client::new(None);
let bytes = client.fetch_annual_csv()?;
let measure = client.measure(&bytes, "34023", "Premature Death")?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

The client currently covers the public annual analytic CSV endpoint [`analytic_data2025_v3.csv`](https://www.countyhealthrankings.org/sites/default/files/media/document/analytic_data2025_v3.csv), including county/state FIPS rows and numeric raw-value/confidence-interval columns. The API has no authentication or API key. Pass `Some(cache_dir)` to `Client::new` to opt into a SHA-256 content-addressed cache; set `CHR_DATA_OFFLINE=1` to require a cache hit. Fetch failures retry transport errors, HTTP 429, and 5xx responses with bounded jitter.

## Terms

Reviewed 2026-11-22: [CHR&R Terms of Use](https://www.countyhealthrankings.org/terms-use) (last updated December 16, 2024) and [data documentation](https://www.countyhealthrankings.org/health-data/methodology-and-sources/data-documentation).

| Question | Finding |
| --- | --- |
| Automated access allowed? | No automated-access rule is stated in the terms. The terms license access to password-free website portions for personal or nonprofit educational, research, or public-health use; this client only accesses the public CSV URL. |
| Redistribution of data | No blanket redistribution license is granted. The content is reserved to CHR&R or its licensors, and underlying-source terms also apply. Commercial copying requires prior written consent. |
| Attribution required | Preserve notices; cite University of Wisconsin Population Health Institute, County Health Rankings & Roadmaps, www.countyhealthrankings.org. Do not imply endorsement. |
| Rate limits | No numeric rate limit is stated. Use modest request frequency; the client caches only when a caller provides a directory. |

This crate redistributes client code, not downloaded data. Use the source data only in ways permitted by the terms and applicable underlying-source restrictions.
