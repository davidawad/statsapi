# statsapi-eurostat

`statsapi-eurostat` is a reusable blocking Rust client for the Eurostat
dissemination API (JSON-stat 2.0) and the GISCO NUTS boundary files. It has no
repository dependencies.

## Usage

This example is also available as [`examples/basic.rs`](examples/basic.rs):

```rust,no_run
use statsapi_eurostat::{Client, NutsLevel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    if let Some(region) = client.nuts_for_point(48.1371, 11.5754, NutsLevel::Two, 2024)? {
        let filters = [("geo", region.id.as_str()), ("unit", "PPS_EU27_2020_HAB")];
        if let Some(observation) = client.latest("nama_10r_2gdp", &filters)? {
            println!("{} {}: {}", region.name, observation.time, observation.value);
        }
    }
    Ok(())
}
```

## Authentication

Both services are public and keyless. The client reads no environment
variables.

## Endpoints and data

- `GET https://ec.europa.eu/eurostat/api/dissemination/statistics/1.0/data/{dataset}?format=JSON&lang=en&{dimension}={code}&lastTimePeriod=6`
  returns JSON-stat. `Client::latest` requires the filters to pin every
  dimension except time and returns the newest non-null period with its status
  flag, region label and the dataset's last-update time.
- `GET https://gisco-services.ec.europa.eu/distribution/v2/nuts/geojson/NUTS_RG_10M_{year}_4326_LEVL_{2|3}.geojson`
  is read into memory, searched with a point-in-polygon test
  (`Client::nuts_for_point`), and dropped. The client never writes to disk and
  has no cache; each call is a live request. Requests retry transport errors,
  HTTP 429 and 5xx with bounded exponential delay.

## Terms

| Service | Access | Licence and terms |
|---|---|---|
| Eurostat dissemination API | keyless, no stated rate limit; the client makes one request per observation | Eurostat data are reusable free of charge with source acknowledgement under [Commission Decision 2011/833/EU](https://ec.europa.eu/eurostat/about-us/policies/copyright) (CC BY 4.0 equivalent) |
| GISCO NUTS boundaries | keyless static files | GISCO statistical units: free reuse with the attribution "© EuroGeographics for the administrative boundaries" ([terms](https://ec.europa.eu/eurostat/web/gisco/geodata/statistical-units)) |
