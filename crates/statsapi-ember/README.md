# statsapi-ember

Reusable blocking Rust client for Ember's public electricity generation data
(generation mix, demand, emissions intensity; yearly and monthly, by country).
No repo dependencies; GPL-3.0-or-later; `publish = false`.

```rust
use statsapi_ember::{latest, Dataset, EmberClient, Field};

let client = EmberClient::new()?;
let (url, rows) = client.fetch_country(Dataset::Yearly, "USA")?;
if let Some((row, share)) = latest(&rows, "Coal", Field::Share) {
    println!("{url}: coal {share}% in {}", row.period);
}
```

Runnable form: `examples/basic.rs`.

## Access path

Ember's keyed API (`https://api.ember-energy.org/v1/...`) answers keyless
calls with HTTP 403 `Invalid API key`, so the client streams the public release
CSVs instead, through a CSV reader that keeps only the requested ISO alpha-3
country. Nothing is written to disk and there is no cache: each call reads the
live file (about 2 MB yearly, 4.5 MB monthly). Callers that report provenance
should call this path `stream`.

## Terms

| Item | Value |
|---|---|
| Publisher | Ember (ember-energy.org) |
| License | CC BY 4.0; attribute Ember |
| Key | none for the CSV downloads |
| Files | `https://files.ember-energy.org/public-downloads/generation/outputs/release_generation_{yearly,monthly}_global.csv` |
| Automated access | The data page links the files directly and publishes no rate limit; the client sends one GET per dataset per call with a descriptive user agent |
| Terms page | <https://ember-energy.org/data/api/> and <https://ember-energy.org/creative-commons/> (checked 2026-11-23) |
