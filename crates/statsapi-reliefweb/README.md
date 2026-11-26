# statsapi-reliefweb

`statsapi-reliefweb` reads disasters and report counts per country from the ReliefWeb API v2 (UN OCHA).

## Install

```toml
[dependencies]
statsapi-reliefweb = { git = "https://github.com/davidawad/statsapi", package = "statsapi-reliefweb" }
```

## Usage

```rust
use statsapi_reliefweb::{Client, DisasterFilter, Status};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = DisasterFilter {
        statuses: vec![Status::Ongoing, Status::Alert],
        ..DisasterFilter::default()
    };
    let (found, origin) = Client::new().disasters("PHL", &filter)?;
    println!("{} ongoing disasters via {}", found.len(), origin.as_str());
    Ok(())
}
```

Auth: no key, but API v2 requires an approved `appname` query parameter (API v1 is retired and answers HTTP 410). There is no default: request your own appname (https://apidoc.reliefweb.int/parameters#appname) and set `RELIEFWEB_APPNAME` or use `Client::with_appname`; without one, requests return `Error::MissingAppname`. Endpoints covered: `POST https://api.reliefweb.int/v2/disasters` and `/v2/reports` (filters `country.iso3`, `status`, `date.event`, `date.created`). The client retries 429, 5xx and transport errors with jitter. Live API first; `Client::with_cache(dir, ttl)` adds a fallback cache read only when the live request fails (TTL capped at 7 days, responses over 8 MiB are not stored). `Origin` says which path answered.

## Terms

| Source | Automated access | Redistribution and attribution | Rate limits |
|---|---|---|---|
| [ReliefWeb](https://reliefweb.int/) (UN OCHA) | Public API; v2 needs an approved appname ([apidoc.reliefweb.int](https://apidoc.reliefweb.int/)). | Content is credited to its original publishers; cite ReliefWeb and the disaster page URL. See the [ReliefWeb terms of use](https://reliefweb.int/terms-conditions). | 1000 calls per day, 1000 entries per call per the API docs. |

Fixtures are synthesized, not copied from upstream.
