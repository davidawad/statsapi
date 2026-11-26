# statsapi-senate-lda

`statsapi-senate-lda` is a reusable blocking Rust client for the US Senate
Lobbying Disclosure Act (LDA) REST API at <https://lda.senate.gov/api/>. It has
no repository dependencies.

## Install

```toml
[dependencies]
statsapi-senate-lda = { git = "https://github.com/davidawad/statsapi", package = "statsapi-senate-lda" }
```

## Usage

See [`examples/basic.rs`](examples/basic.rs):

```rust
use statsapi_senate_lda::{last_four_quarters, FilingQuery, LdaClient};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = LdaClient::new()?;
    let mut total = 0;
    for period in last_four_quarters("2026-11-23")? {
        total += client.count(&FilingQuery::new(period).client_state("NJ")?)?;
    }
    println!("NJ client filings, last four quarters: {total}");
    Ok(())
}
```

No key is read: the anonymous tier is used. `count` asks for a one-row page and
reads the response's `count`; `summarize` pages through filings (25 per page)
and totals reported spending (firm `income` plus in-house `expenses`) and the
filings per general issue area. A query that needs more pages than the caller's
budget fails with `ClientError::Budget` before reading further. Nothing is
cached or written to disk. HTTP 429 honours `Retry-After` (capped at 30 s);
5xx and transport errors retry with backoff.

Queries read original quarterly reports only (`filing_type=Q1`..`Q4`);
amendments (`Q1A`, ...) are excluded so a filing is not counted twice.

## Terms

Checked 2026-11-23 against the LDA API documentation at
<https://lda.senate.gov/api/redoc/v1/> and the Senate Office of Public Records
disclosure pages. The pod that wrote this client could not reach any
`senate.gov` host (see Verification), so the table restates the published
terms from the documentation as last read; re-check them against the live
pages before relying on it.

| Question | Finding |
| --- | --- |
| Automated access allowed? | Yes. The REST API is the documented machine interface; an anonymous tier needs no key and a registered key raises the limits. |
| Redistribution of data | Lobbying disclosure filings are public records of the US Senate; no redistribution restriction is stated for the filings. The data may not be used to solicit contributions or for commercial solicitation (2 U.S.C. 1605(c)). |
| Attribution required? | Not required; cite the Senate Office of Public Records as the source. |
| Rate limits | Anonymous tier is throttled to a small number of requests per minute (documented as 15/min; 120/min with a key). The API answers HTTP 429 with `Retry-After`. |

## Verification

Offline tests use a hand-built fixture shaped like the documented
`/api/v1/filings/` response (`tests/fixtures/filings-page.json`, synthetic
values, not copied from the API). The response shape and the `filing_type`,
`client_state` and `registrant_state` filter names are taken from the API
documentation and have not been confirmed against a live response.

On 2026-11-23 every request from the build pod to `lda.senate.gov`,
`lda.gov` and `www.senate.gov` returned HTTP 403 `Access Denied` from the
Akamai edge, while other hosts answered. The ignored live test therefore fails
in that pod with `LDA API answered HTTP 403`; run
`cargo test -p statsapi-senate-lda -- --ignored` from a host the Senate edge
accepts.
