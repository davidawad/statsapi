# statsapi-gleif

`statsapi-gleif` is a reusable blocking Rust client for counting LEI records in the [GLEIF API v1](https://api.gleif.org/docs/v1).

## Usage

```rust
use statsapi_gleif::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = Client::new()?.count(&[("entity.legalAddress.country", "US")])?;
    println!("{} US legal entities", count.total);
    Ok(())
}
```

## Authentication

None. The API is keyless. The client keeps nothing on disk: every call is a live request (a page of one record; the count is `meta.pagination.total`). Requests retry transport errors, HTTP 429 and 5xx with bounded exponential delay and jitter.

## Endpoints and filters

`GET /api/v1/lei-records?filter[...]=...&page[size]=1`. Filters verified 2026-11-23: `entity.legalAddress.country` (ISO 3166-1 alpha-2), `entity.jurisdiction` (e.g. `US-NJ`), `registration.status` (`ISSUED`, `LAPSED`, ...), `registration.initialRegistrationDate` (`YYYY-MM-DD..YYYY-MM-DD`). The API rejects `entity.legalAddress.region` and `.city` filters, so subdivisions are reachable only through the legal jurisdiction.

## Terms

Terms reviewed 2026-11-23 at [GLEIF Terms of Use](https://www.gleif.org/en/meta/terms-of-use) and the [LEI data license](https://www.gleif.org/en/lei-data/gleif-concatenated-file/license).

| Question | Terms summary |
| --- | --- |
| Automated access allowed? | Yes. The API is public, keyless and documented for programmatic use. |
| Redistribution of data | LEI data is published under CC0 1.0; redistribution is unrestricted. This crate distributes no upstream data. |
| Attribution required? | No. Citing GLEIF is courteous. |
| Rate limits | The API documents 60 requests per minute per client; respect HTTP 429. |

## Verified

`cargo test -p statsapi-gleif` and `cargo test -p statsapi-gleif -- --ignored` on 2026-11-23.
