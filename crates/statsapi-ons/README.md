# statsapi-ons

`statsapi-ons` is a reusable blocking Rust client for the UK Office for National Statistics dataset API and for postcodes.io point-to-local-authority lookup.

## Install

```toml
[dependencies]
statsapi-ons = { git = "https://github.com/davidawad/statsapi", package = "statsapi-ons" }
```

## Usage

This example is also available as [`examples/basic.rs`](examples/basic.rs):

```rust
use statsapi_ons::{latest, Client};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let version = client.version("mid-year-pop-est", None)?;
    let order = client.options(&version, "time")?;
    let rows = client.observations(
        &version,
        &[("time", "*"), ("geography", "E09000033"), ("age", "total"), ("sex", "all")],
    )?;
    if let Some(row) = latest(&rows, &order) {
        println!("{}: {}", row.time, row.value);
    }
    Ok(())
}
```

## Authentication

None. Both services are keyless. No environment variables are read. The client keeps no cache and writes nothing to disk; every call is a live request.

## Endpoints covered

- `GET /v1/datasets/{id}` and `/v1/datasets/{id}/editions/{edition}` resolve a dataset's latest version.
- `GET {version}/dimensions/{dimension}/options` lists option ids. ONS lists time options newest first, and observation order is arbitrary, so `latest` ranks observations by that list.
- `GET {version}/observations?time=*&geography=...` returns observations; at most one dimension may be the `*` wildcard. A bad selection is HTTP 400 with a one-line reason, surfaced in `Error::HttpStatus`.
- `GET https://api.postcodes.io/postcodes?lon=&lat=&limit=1&radius=2000` reverse-geocodes a point to its local-authority district (`codes.admin_district`).

Requests retry transport errors, HTTP 429 and 5xx with bounded exponential delay and jitter.

## Terms

Reviewed 2026-11-23: [ONS developer hub](https://developer.ons.gov.uk/), [Open Government Licence v3.0](https://www.nationalarchives.gov.uk/doc/open-government-licence/version/3/), [postcodes.io](https://postcodes.io/docs).

| Question | Terms summary |
| --- | --- |
| Automated access allowed? | Yes. The ONS API is a public, keyless programmatic interface built for this. postcodes.io is a free public API. |
| Redistribution of data | ONS data is under the Open Government Licence v3.0 (reuse including commercially). postcodes.io serves ONS/Ordnance Survey open data under OGL too. This crate ships no upstream data. |
| Attribution required? | Yes: "Contains public sector information licensed under the Open Government Licence v3.0", plus the ONS as source. |
| Rate limits | ONS documents a limit of about 120 requests per 10 seconds (HTTP 429 above it); postcodes.io asks for reasonable use. The client retries 429 with backoff. |

## Verified

2026-11-23: `cargo test -p statsapi-ons` (5 unit tests over synthesized fixtures) and `cargo test -p statsapi-ons -- --ignored` (live: Westminster population; reverse-geocode of 51.5072, -0.1276 to E09000033) pass. Fixture values are synthetic.
