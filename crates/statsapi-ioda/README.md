# statsapi-ioda

`statsapi-ioda` is a blocking client for the
[IODA HTTP API](https://api.ioda.inetintel.cc.gatech.edu/v2/) (Georgia Tech
Internet Intelligence Research Lab). It queries the keyless API live and never
caches or writes to disk.

## Usage

```rust
use statsapi_ioda::{Client, EntityType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let now = 1_795_428_000;
    let episodes = Client::new()?.episodes(EntityType::Country, "IQ", now - 30 * 86_400, now)?;
    println!("{} episodes in 30 days", episodes.len());
    Ok(())
}
```

- `episodes` reads `outages/events` and merges fragments that overlap or sit
  within an hour of each other into one `Episode` (start, end, entity,
  datasources, title).
- `signal` reads `signals/raw/<type>/<code>` for the `bgp` or `ping-slash24`
  (active probing) datasource; `compare` returns the latest non-missing value
  against the median of samples at least six hours older, so an 8-day window
  gives a 7-day baseline.
- `region_code` maps a region name (for example a US state) to IODA's numeric
  region code.

Requests retry transient transport errors and HTTP 429/5xx up to three attempts.

## Terms

| Source | Automated access | Redistribution and attribution | Rate limits |
|---|---|---|---|
| [IODA API](https://api.ioda.inetintel.cc.gatech.edu/v2/) | Public keyless HTTP API documented for programmatic use. | Data is Copyright Georgia Tech Research Corporation (stated in every response). No separate redistribution licence is published on the API; credit "IODA, Georgia Tech Internet Intelligence Research Lab" wherever values are shown and use it for non-commercial analysis. | None numerically stated; keep requests small. |
