# statsapi-usgs-earthquakes

Blocking client for the [USGS Earthquake Catalog](https://earthquake.usgs.gov/fdsnws/event/1/) FDSN event API. No repo dependencies. It queries the live API and persists nothing.

## Usage

```rust
use statsapi_usgs_earthquakes::{Area, Client, Order, Query};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let query = Query {
        area: Area::Radius { lat: 40.54, lon: -74.36, km: 100.0 },
        start: "2016-11-23".into(),
        min_magnitude: Some(3.0),
    };
    let client = Client::new();
    let (count, _url) = client.count(&query, "2026-11-23")?;
    let (strongest, _url) = client.top_event(&query, "2026-11-23", Order::Magnitude)?;
    println!("{count} events; strongest {strongest:?}");
    Ok(())
}
```

`Area::Radius` and `Area::Bbox` map to the API's circle and rectangle filters. Transient transport failures and HTTP 429/5xx are retried three times.

## Terms

| Source | Automated access | Redistribution and attribution | Rate limits |
|---|---|---|---|
| [USGS Earthquake Catalog](https://earthquake.usgs.gov/fdsnws/event/1/) | Public keyless REST API; intended for programmatic use. | U.S. Government work, public domain; credit USGS. | Queries returning over 20,000 events are rejected (`count` has no such limit); no numeric request limit stated. |
