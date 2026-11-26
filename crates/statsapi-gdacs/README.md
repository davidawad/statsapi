# statsapi-gdacs

`statsapi-gdacs` reads active and recent orange and red disaster alerts (earthquake, tropical cyclone, flood, volcano, drought, wildfire) from the keyless GDACS API.

## Install

```toml
[dependencies]
statsapi-gdacs = { git = "https://github.com/davidawad/statsapi", package = "statsapi-gdacs" }
```

## Usage

```rust
use statsapi_gdacs::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let today = chrono::Utc::now().date_naive();
    let (alerts, origin) = Client::new().alerts(today - chrono::Duration::days(30), today)?;
    println!("{} orange/red alerts via {}", alerts.len(), origin.as_str());
    Ok(())
}
```

Auth: none. Endpoint covered: `https://www.gdacs.org/gdacsapi/api/events/geteventlist/SEARCH` (`eventlist`, `alertlevel`, `fromDate`, `toDate`). The client retries 429, 5xx and transport errors with jitter. Live API first; `Client::with_cache(dir, ttl)` adds a fallback cache read only when the live request fails (TTL capped at 7 days, responses over 8 MiB are not stored). `Origin` says which path answered.

## Terms

| Source | Automated access | Redistribution and attribution | Rate limits |
|---|---|---|---|
| [GDACS](https://www.gdacs.org/) (EC JRC and UN OCHA) | Public keyless API, documented at [gdacs.org/gdacsapi](https://www.gdacs.org/gdacsapi/). | Free to use; credit GDACS and cite the event report URL. See the [GDACS disclaimer](https://www.gdacs.org/About/disclaimer.aspx). | None stated; keep requests infrequent. |

Fixtures are synthesized, not copied from upstream.
