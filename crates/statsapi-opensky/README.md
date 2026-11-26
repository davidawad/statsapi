# statsapi-opensky

`statsapi-opensky` is a reusable blocking Rust client that counts the aircraft the OpenSky Network sees inside a bounding box right now.

## Install

```toml
[dependencies]
statsapi-opensky = { git = "https://github.com/davidawad/statsapi", package = "statsapi-opensky" }
```

## Usage

See [`examples/basic.rs`](examples/basic.rs):

```rust
use statsapi_opensky::{BoundingBox, OpenSkyClient};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bbox = BoundingBox::around(40.7128, -74.006, 100.0)?;
    let (url, snapshot) = OpenSkyClient::new()?.snapshot(&bbox)?;
    println!("{url}");
    println!("{} aircraft, {} on the ground at {}", snapshot.total, snapshot.on_ground, snapshot.time);
    Ok(())
}
```

## Auth

Anonymous only; no key or environment variable is read. Retries use jitter
on HTTP 429, 5xx and transport errors; a persistent 429 surfaces as
`ClientError::RateLimited`. Nothing is cached or written to disk: every call
is a live request.

## Endpoints covered

- `GET /api/states/all?lamin&lomin&lamax&lomax` — live state vectors, counted
  by `parse_snapshot` (total, `on_ground`).

Not covered: `/flights/arrival` and `/flights/departure`. On 2026-11-23 an
anonymous request answered `You cannot access historical flights`, so
airport arrivals and departures need an authenticated account.

## Terms

| Topic | Terms |
|---|---|
| Automated access | Permitted through the REST API; anonymous use is limited to the most recent state vectors |
| Rate limits | Anonymous: 400 API credits per day, 10 s time resolution; a box of up to 25 deg² costs 1 credit, larger boxes cost more (HTTP 429 when exhausted) |
| Redistribution | Data is for research and non-profit use; commercial use needs a licence from OpenSky |
| Attribution | Cite Schäfer et al., "Bringing Up OpenSky", IPSN 2014 |
| Upstream terms | <https://opensky-network.org/about/terms-of-use> and <https://openskynetwork.github.io/opensky-api/rest.html> |
