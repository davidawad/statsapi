# statsapi-launch-library

`statsapi-launch-library` is a reusable blocking Rust client that counts orbital launches by provider country, date and outcome, and finds the next scheduled launch, from The Space Devs Launch Library 2.

## Install

```toml
[dependencies]
statsapi-launch-library = { git = "https://github.com/davidawad/statsapi", package = "statsapi-launch-library" }
```

## Usage

See [examples/basic.rs](examples/basic.rs) for a compiling example:

```rust,no_run
use statsapi_launch_library::{Client, Filter, Listing};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder("my-indicator/1.0 (contact: ops@example.org)").build()?;
    let providers = client.providers("USA")?;
    let filter = Filter {
        provider_ids: providers.ids.clone(),
        net_from: Some("2026-01-01".into()),
        ..Filter::default()
    };
    let launches = client.count(Listing::Previous, &filter)?;
    println!("{} launches ({})", launches.count, launches.source_url);
    Ok(())
}
```

## Authentication

The covered endpoints are keyless. Send a descriptive User-Agent with contact information. There is no API key or credential environment variable.

## Endpoints covered

- `GET /2.2.0/launch/previous/` and `GET /2.2.0/launch/upcoming/` with `limit=1&mode=list`, filtered by `lsp__ids` (launch service provider agencies), `net__gte` and `status__ids` (3 success, 4 failure, 7 partial failure). Counts come from the response `count`, so a question costs one request regardless of how many launches match.
- `GET /2.2.0/agencies/?country_code=<ISO3>&mode=list&limit=100` (up to 5 pages) resolves a country to its provider agency ids; agencies listing several countries are dropped. The launch endpoints have no provider-country filter (`lsp__country_code` is silently ignored), so country questions go through these ids.
- The first upcoming result supplies the next launch's name and `net`.

## Retries and cache

Blocking reqwest with rustls; transport errors, HTTP 429 and 5xx are retried with jittered backoff, but a `Retry-After` over 5 seconds fails fast (the free tier throttles for most of an hour). A caller may enable `fallback_cache(dir, ttl)`: each live answer is stored keyed by the SHA-256 of its URL, and a stored answer is served (reported as `Origin::Cache`) only when the live call failed and the entry is younger than `ttl` (clamped to 7 days). The cache holds at most 4 MiB and prunes expired and oldest entries. There is no global state.

## Terms

Terms reviewed 2026-11-23.

| Question | Launch Library 2 |
| --- | --- |
| Automated access allowed? | Yes; the API is public and keyless, with a throttled free tier. |
| Redistribution of data? | Data is community-curated; The Space Devs permit use with attribution (see the linked terms). Derived counts are fine; do not mirror the database. |
| Attribution required? | Yes. Credit "The Space Devs, Launch Library 2". |
| Rate limits | Free tier about 15 requests per hour per IP; a paid API key raises it. Requests over the limit return HTTP 429 with `Retry-After`. |

Sources: [Launch Library 2 API docs](https://ll.thespacedevs.com/docs/) and [The Space Devs terms and API throttling notes](https://thespacedevs.com/llapi).

## Verified

- Method: offline tests against a local mock server and synthesized fixtures; one ignored live test.
- Date: 2026-11-23.
