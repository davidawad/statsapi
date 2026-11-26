# statsapi-nasa-power

`statsapi-nasa-power` is a blocking Rust client for the keyless NASA POWER daily point API: temperature, precipitation and solar irradiance for any latitude/longitude.

## Install

```toml
[dependencies]
statsapi-nasa-power = { git = "https://github.com/davidawad/statsapi", package = "statsapi-nasa-power" }
```

## Usage

```rust
use statsapi_nasa_power::{Client, DailyQuery, Date};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let daily = Client::new().daily_point(&DailyQuery {
        latitude: 40.5434,
        longitude: -74.3626,
        start: Date::parse("20250101")?,
        end: Date::parse("20250107")?,
        parameters: &["T2M"],
    })?;
    println!("{:?}", daily.values("T2M"));
    Ok(())
}
```

No key is needed. The client retries HTTP 429, 5xx and transport errors with jittered backoff, drops POWER's `-999` fill values, and keeps nothing on disk (no cache). Endpoint covered: `https://power.larc.nasa.gov/api/temporal/daily/point` (community `AG`).

## Terms

| Source | Automated access | Redistribution and attribution | Rate limits |
|---|---|---|---|
| [NASA POWER](https://power.larc.nasa.gov/docs/services/api/) | Public REST API, no key; automated use is the intended purpose. | [Data use policy](https://power.larc.nasa.gov/docs/referencing/): freely available, cite "NASA Prediction Of Worldwide Energy Resources (POWER)". | Fair use; the service answers 429 under load and the client backs off. |

Fixtures are synthesized, not copies of upstream values.
