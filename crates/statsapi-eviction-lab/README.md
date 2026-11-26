# statsapi-eviction-lab

Fetch and decode Eviction Lab's public Eviction Tracking System aggregate CSV files from a reusable blocking Rust client.

## Install

```toml
[dependencies]
statsapi-eviction-lab = { git = "https://github.com/davidawad/statsapi", package = "statsapi-eviction-lab" }
```

## Usage

The same example is in [examples/basic.rs](examples/basic.rs) and compiles with cargo build -p statsapi-eviction-lab --example basic.

```rust
use statsapi_eviction_lab::{Client, NetworkPolicy};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(None)?;
    let url = "https://dev.evictionlab.org/uploads/philadelphia_weekly_2020_2021.csv";
    let csv = client.fetch(url, NetworkPolicy::Allow)?;
    let latest = client.latest_week(&csv, "filings_avg", "")?;
    println!("week ending {}: current aggregate {}", latest.week_date, latest.current);
    Ok(())
}
```

## Authentication and cache

The ETS files covered here are public and keyless. No credential or key environment variable is read. Client::new(Some(path.into())) enables a caller-owned cache; Client::new(None) disables it. NetworkPolicy::CacheOnly uses only a cached response and returns Error::CacheMiss otherwise. Requests retry transient transport failures, HTTP 429 and 5xx responses with bounded jitter.

The client fetches weekly city files named {city}_weekly_2020_2021.csv and all-states data from allstates_weekly_2020_2021.csv under https://dev.evictionlab.org/uploads/. It decodes the upstream ETS CSV layout, including current filings and comparison columns. It does not cover gated national estimate downloads from the email form.

## Terms

Reviewed 2026-11-22. The ETS [Get the Data page](https://evictionlab.org/eviction-tracking/get-the-data/) presents downloadable aggregate files, encourages reuse, and asks users to cite the published release. It does not state a data license or rate limit. This crate redistributes client code, not upstream datasets.

| Question | Finding |
| --- | --- |
| Automated access allowed? | Public download page includes code examples for using its files; no prohibition on automated retrieval found. No stated rate limit. |
| Redistribution of data | Not specified for ETS aggregate files. This crate does not redistribute data. |
| Attribution required | Cite the Eviction Tracking System release as shown on the upstream download page: Peter Hepburn, Adam Chapnik, Danny Grubbs-Donovan, Jacob Haas, Grace Hartley, Olivia Jin, Sarah Johnson, Renee Louis, Jasmine Rangel, Salar Safarkhani, and Matthew Desmond, *Eviction Tracking System: Version 4.0*. Princeton: Princeton University, 2026. |
| Rate limits | No numeric limit stated on the upstream terms/download page; retry delay is bounded and jittered. |

