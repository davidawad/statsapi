# statsapi-census-bds

A blocking Rust client for the U.S. Census Bureau Business Dynamics Statistics (BDS) timeseries API: establishment birth and death rates and job creation and destruction rates for the US, states and counties.

## Install

```toml
statsapi-census-bds = { git = "https://github.com/davidawad/statsapi" }
```

## Usage

```rust
use statsapi_census_bds::{Client, Geography};

let client = Client::new(std::env::var("CENSUS_API_KEY").ok(), None)?;
let o = client.newest(&Geography::County("10".into(), "003".into()))?;
println!("{}: {:?}", o.year, o.get("JOB_CREATION_RATE"));
```

A runnable version is `examples/basic.rs`.

## Auth

`api.census.gov` now redirects calls without a `key` to its missing-key page (observed 2026-11-23, including ACS). The client reports that as `credential_required`; pass the key from the caller (`CENSUS_API_KEY`). Keys are free at <https://api.census.gov/data/key_signup.html>. The key is never included in errors or in `Observation::url`.

## Endpoints covered

`GET https://api.census.gov/data/timeseries/bds` with `get=NAME,YEAR,<measures>`, `for=us|state|county`, `NAICS=00` (all sectors). Measures: `ESTABS_ENTRY_RATE`, `ESTABS_EXIT_RATE`, `JOB_CREATION_RATE`, `JOB_DESTRUCTION_RATE`, `NET_JOB_CREATION_RATE`, `ESTAB`. `Client::newest` returns the newest year with published values.

## Access order and cache

Live API first, with jittered retry on 429, 5xx and transport errors. A response cache (caller-supplied directory, entries at most 7 days old, directory capped at 8 MiB) is read only when the live call fails; `Observation::source` says `api` or `cache`. No bulk files are downloaded.

## Terms

| Item | Terms |
| --- | --- |
| Automated access | Permitted with an API key ([Census API terms](https://www.census.gov/data/developers/about/terms-of-service.html)) |
| Redistribution | BDS is a federal statistical product; public domain, no restriction |
| Attribution | "This product uses the Census Bureau Data API but is not endorsed or certified by the Census Bureau." |
| Rate limits | 500 queries per day per IP without a key historically; none published with a key |
