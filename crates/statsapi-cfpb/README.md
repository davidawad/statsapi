# statsapi-cfpb

`statsapi-cfpb` gives Rust applications a small blocking client for CFPB complaint and consumer credit trend data.

## Install

```toml
[dependencies]
statsapi-cfpb = { git = "https://github.com/davidawad/statsapi", package = "statsapi-cfpb" }
```

## Use

The complete compiling example is [`examples/basic.rs`](examples/basic.rs).

```rust,no_run
use statsapi_cfpb::CfpbClient;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let client = CfpbClient::new(None)?;
    let totals = client.complaints(
        "https://www.consumerfinance.gov/data-research/consumer-complaints/search/api/v1/?size=0&date_received_min=2025-01-01&date_received_max=2025-12-31",
    )?;
    println!("{} complaints", totals.total);
    Ok(())
}
```

Authentication: the Consumer Complaint Database API and Consumer Credit Trends files are public and require no key. The client has no credential env var. Pass `Some(cache_path)` to `CfpbClient::new` to use a caller-managed cache; call `.offline()` to require cached responses. Transient transport errors, HTTP 429, and 5xx responses receive bounded exponential retry with jitter.

## Endpoints and files

- Consumer Complaint Database search API: caller supplies a complete filtered URL; `complaints` decodes the hit total and product aggregation.
- CFPB Consumer Credit Trends state map CSVs in `cfpb/consumer-credit-trends-data`: `state_map_value` reads one state row. The crate does not construct loan-specific paths, so callers can choose the published map file.

## Terms

Terms checked on 2026-11-22 against the [CFPB Consumer Complaint Database page](https://www.consumerfinance.gov/data-research/consumer-complaints/) and its [API documentation](https://cfpb.github.io/api/ccdb/). CFPB says published complaint data is freely available for anyone to use, analyze, and build on. The API documentation invites developers to build tools. No separate API rate limit or attribution requirement was published on those pages; the client retries transient failures and uses conservative bounded backoff.

| Question | Finding |
| --- | --- |
| Automated access allowed? | Yes. CFPB documents the API for building tools and offers public API access. |
| Redistribution of data? | Yes for complaint data: CFPB says it is freely available for anyone to use, analyze, and build on. Consumer Credit Trends maps are public CFPB-published data files. |
| Attribution required? | No explicit requirement found on the cited pages; attribution to CFPB is recommended. |
| Rate limits | No published numeric limit found on the cited pages. Use bounded retries and avoid excessive request volume. |

## Tests

Offline fixtures preserve the upstream JSON and CSV field layout while using wholly synthesized complaint categories, counts, and state codes. The live integration test is ignored by default. CFPB complaint volume is not a statistical sample of consumers' marketplace experiences; interpret counts with the population and coverage caveats on the upstream page.

