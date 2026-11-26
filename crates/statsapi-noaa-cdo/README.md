# statsapi-noaa-cdo

`statsapi-noaa-cdo` is a pure blocking client for the keyless NOAA NCEI Access
Data Service: nearest GHCN-Daily stations to a point, daily summaries, and
1991-2020 daily normals. It has no repo dependencies and writes nothing to
disk: every call is a live request (source order 1, `api`). Distinct from
`statsapi-noaa`, which reads Storm Events bulk files.

## Usage

```rust
use statsapi_noaa_cdo::Client;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let (start, end) = ("2026-10-01".parse()?, "2026-11-01".parse()?);
    let stations = client.stations_near(40.74, -74.17, 0.25, start, end)?;
    let days = client.daily_summaries(&stations[0].id, start, end, "PRCP,TMAX,TMIN")?;
    println!("{} days at {}", days.len(), stations[0].id);
    Ok(())
}
```

## Terms

| Source | Automated access | Redistribution and attribution | Rate limits |
|---|---|---|---|
| [NCEI Access Data Service](https://www.ncei.noaa.gov/access/services/data/v1) and [search service](https://www.ncei.noaa.gov/access/services/search/v1/data) | Public, keyless REST APIs intended for programmatic use. | U.S. government public data; credit NOAA NCEI (GHCN-Daily, U.S. Climate Normals 1991-2020). | No numeric limit published; this client issues a handful of requests per lookup and a descriptive User-Agent. |
