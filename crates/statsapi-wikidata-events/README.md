# statsapi-wikidata-events

`statsapi-wikidata-events` is a blocking client for Wikidata event items and publisher links on Wikipedia Current Events.

## Install

```toml
[dependencies]
statsapi-wikidata-events = { git = "https://github.com/davidawad/statsapi", package = "statsapi-wikidata-events" }
```

## Usage

The same flow is compiled from [`examples/basic.rs`](examples/basic.rs). Set `WIKIMEDIA_USER_AGENT` to a descriptive caller-owned value before running it.

```rust,no_run
use std::env;
use chrono::Utc;
use statsapi_wikidata_events::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(&env::var("WIKIMEDIA_USER_AGENT")?)?;
    let events = client.events("USA", Utc::now().date_naive())?;
    println!("{} events", events.len());
    Ok(())
}
```

No API key is required. The user agent is supplied by the caller and should identify the application and a contact method.

## Coverage

| Upstream endpoint or file | Supported access |
| --- | --- |
| Wikidata Query Service | Country event items, dates, types, labels, and optional fatalities over the preceding year |
| Wikipedia Current Events | External publisher links for a selected daily page |
| Event CSV | Flexible synthetic or cached event CSV parsing into normalized records |

## Wikimedia terms

Terms reviewed 2026-11-26: [Wikidata licensing](https://www.wikidata.org/wiki/Wikidata:Licensing), [Wikimedia Terms of Use](https://foundation.wikimedia.org/wiki/Terms_of_Use/en), and [Wikimedia API usage guidance](https://foundation.wikimedia.org/wiki/Policy:Wikimedia_Foundation_API_Usage_Guidelines/en).

| Question | Terms summary |
| --- | --- |
| Automated access allowed? | Yes, provided clients follow the API etiquette and send a meaningful user agent; automated access must not burden or circumvent Wikimedia services. |
| Redistribution of data | Wikidata structured data is generally CC0; Wikipedia text and linked material have their own licenses, so this client returns links and metadata rather than republishing article text. |
| Attribution required? | CC0 does not require attribution, but acknowledge Wikidata/Wikimedia and preserve source links; Wikipedia content requires the applicable license notices. |
| Rate limits | Follow global Wikimedia limits, `maxlag` guidance where applicable, and any server request to slow down. |
