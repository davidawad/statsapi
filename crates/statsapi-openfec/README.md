# statsapi-openfec

A reusable blocking Rust client for OpenFEC aggregate Schedule A and committee data.

Install it as a git dependency:

```toml
[dependencies]
statsapi-openfec = { git = "https://github.com/davidawad/statsapi", package = "statsapi-openfec" }
```

The same usage is in [`examples/basic.rs`](examples/basic.rs):

```rust,no_run
use statsapi_openfec::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::from_env()?;
    let by_zip = client.schedule_a_by_zip("00000", 2024)?;
    let by_state = client.schedule_a_by_state("XX", 2024)?;
    println!("{} candidate committee rows", by_zip.committees.len());
    println!("state aggregate: {}", by_state.total);
    Ok(())
}
```

## Authentication and cache

Pass an API key with `Client::new(key)`, or set `API_DATA_GOV_KEY` and call
`Client::from_env()`. The API key is redacted from the client's `Debug` output.
`Client::with_cache_dir(path)` enables a caller-owned response cache; no cache
directory or process-global configuration is used by default. Responses are
retried with jitter after HTTP 429, 5xx and transient transport errors.

## Endpoints and data

| Method | OpenFEC path | Returned data |
| --- | --- | --- |
| `schedule_a_by_zip` | `/schedules/schedule_a/by_zip/` | Aggregated Schedule A rows by ZIP and cycle, joined to committee type and party |
| `schedule_a_by_state` | `/schedules/schedule_a/by_state/` | Aggregated Schedule A rows by state and cycle, joined to committee type and party |
| Internal metadata lookup | `/committees/` | Committee ID, committee type and party used to classify aggregate rows |

The client returns committee identifiers and aggregate amounts/counts. It
does not expose individual contributor names or other contributor-level
records. Under the [FEC contributor-information restriction](https://www.fec.gov/updates/sale-or-use-contributor-information/)
and [52 U.S.C. 30111(a)(4)](https://uscode.house.gov/uscode-text?req=granuleid:USC-prelim-title52-section30111),
individual contributor names and information copied from FEC reports may not
be sold or used to solicit contributions or for commercial purposes. The
statute's exception allows use of a political committee's name and address to
solicit that committee. This client is intended for aggregate and committee
analysis; callers must not use individual contributor information for
solicitation or commercial purposes.

## Terms

Terms read 2026-11-22 from the [OpenFEC API documentation](https://api.open.fec.gov/developers/),
its linked [API terms of service](https://github.com/fecgov/FEC/blob/master/TERMS-OF-SERVICE.md),
the [FEC sale-or-use guidance](https://www.fec.gov/updates/sale-or-use-contributor-information/)
and [52 U.S.C. 30111(a)(4)](https://uscode.house.gov/uscode-text?req=granuleid:USC-prelim-title52-section30111).

| Topic | Finding |
| --- | --- |
| Automated access allowed? | Yes. The API documentation describes programmatic API use and provides API keys; use remains subject to the API terms and acceptable-use policy. |
| Redistribution of data | The API terms permit services to search, display, analyze, retrieve and view data, but do not state a blanket redistribution license. The statute restricts sale or use of report information for solicitation or commercial purposes; individual contributor information is not exposed by this client. |
| Attribution required? | Yes. API terms say services accessing the API should identify OpenFEC API as the data source; do not imply FEC endorsement. |
| Rate limits | The API docs list 1,000 calls/hour per API key, up to 100 results/page, and allow requests for 7,200 calls/hour; `DEMO_KEY` is much more limited. Limits may change or be enforced by the FEC. |

## Verification

Verified 2026-11-22 in the repository workspace with the required format,
clippy, tests, ignored live-test, example-build and isolated-crate checks.
The isolated check copied the crate to `/tmp`, added a standalone workspace
with inline workspace lint definitions, and ran `cargo check --offline`.
Offline fixtures retain the OpenFEC JSON response layout and synthesize every
value; no upstream values are copied. No upstream values were fetched during
verification because `API_DATA_GOV_KEY` was unavailable; the ignored live test
printed `credential_missing: API_DATA_GOV_KEY` and exited successfully.

## Verification results

- `cargo fmt -p statsapi-openfec -p fec-data --check`: pass (0 failures)
- `cargo clippy --no-deps -p statsapi-openfec -p fec-data --all-targets -- -D warnings`: pass (0 warnings)
- `cargo test -p statsapi-openfec -p fec-data`: pass (6 passed, 0 failed, 2 ignored)
- `cargo test -p statsapi-openfec -- --ignored`: pass (1 passed, 0 failed; credential missing, no network request)
- `cargo build -p statsapi-openfec --example basic`: pass (0 failures)
- Standalone `cargo check --offline`: pass (0 failures; no repository path dependencies)
