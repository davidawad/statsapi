# statsapi-nws

`statsapi-nws` reads active weather alerts and gridpoint forecasts for any US latitude/longitude from the NOAA National Weather Service API, without a key.

## Install

```toml
[dependencies]
statsapi-nws = { git = "https://github.com/davidawad/statsapi", package = "statsapi-nws" }
```

## Usage

```rust
use statsapi_nws::Client;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new("example-app (you@example.com)");
    let alerts = client.alerts_active(33.58, -101.85)?;
    println!("{} active alerts", alerts.len());
    Ok(())
}
```

## Auth

None. NWS requires a descriptive `User-Agent` that identifies the application and a contact; pass it to `Client::new`.

## Endpoints covered

- `GET /alerts/active?point=lat,lon` decoded to `Alert` (event, CAP severity, headline, expiry).
- `GET /points/lat,lon` then the returned `forecast` URL, decoded to `Forecast` periods with temperatures normalized to degrees Fahrenheit; `Forecast::high_on` / `low_on` pick a local date's high and low.

Requests retry with jitter on 429, 5xx and transport errors. The client keeps no cache: alerts are only meaningful live.

## Terms

| Source | Automated access | Redistribution and attribution | Rate limits |
|---|---|---|---|
| [NWS API](https://www.weather.gov/documentation/services-web-api) | Free, keyless public API; a descriptive `User-Agent` is required. | US federal public-domain data; credit NOAA/NWS. See [disclaimer](https://www.weather.gov/disclaimer). | Not published numerically; "reasonable" use, retry after a short wait on 429. |

Fixtures keep the upstream layout with synthesized values.
