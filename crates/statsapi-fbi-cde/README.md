# statsapi-fbi-cde

A reusable blocking Rust client for the FBI Crime Data Explorer (CDE) summarized-offense API: monthly violent and property crime rates per 100,000 people, offense counts and population for a state or a reporting agency.

Install it as a git dependency:

```toml
[dependencies]
statsapi-fbi-cde = { git = "https://github.com/davidawad/statsapi", package = "statsapi-fbi-cde" }
```

The same usage is in [`examples/basic.rs`](examples/basic.rs):

```rust,no_run
use statsapi_fbi_cde::{year_sum, Client, Offense};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::from_env()?;
    let summary = client.state_summary("NJ", Offense::Violent, 2022, 2023)?;
    if let Some((name, monthly)) = summary.series_other_than(&[]) {
        println!("{name} 2023 violent crime per 100k: {:?}", year_sum(monthly, 2023));
    }
    println!("{} agencies in NJ", client.agencies_by_state("NJ")?.len());
    Ok(())
}
```

## Authentication and caching

An api.data.gov key is required: pass it to `Client::new(key)` or set
`API_DATA_GOV_KEY` and call `Client::from_env()`. The key is sent as the
`API_KEY` query parameter, is redacted from `Debug`, and never appears in the
URLs the client reports (`state_url`, `agency_url`). The client keeps no cache
and no global state: every call is a live request. Responses are retried with
jitter after HTTP 429, 5xx and transient transport errors.

## Endpoints covered

| Method | CDE path | Returned data |
| --- | --- | --- |
| `agencies_by_state` | `/agency/byStateAbbr/{ST}` | ORI, agency name, county, type, per agency |
| `state_summary` | `/summarized/state/{ST}/{violent-crime\|property-crime}` | monthly rate per 100k for the state and the nation |
| `agency_summary` | `/summarized/agency/{ORI}/{violent-crime\|property-crime}` | monthly rate, count and population for the agency, its state and the nation |

`year_sum` adds the twelve monthly rates of a calendar year into an annual
rate per 100,000 (and returns `None` for an incomplete year); `year_mean`
averages monthly population.

## Terms

Sources: the [CDE API documentation](https://cde.ucr.cjis.gov/LATEST/webapp/#/pages/docApi)
and the [api.data.gov terms of service](https://api.data.gov/terms-of-service/).
The rate-limit row was observed on 2026-11-23; the other rows restate the
published terms and were not re-read in that run.

| Topic | Finding |
| --- | --- |
| Automated access allowed? | Yes, with an api.data.gov key; the CDE API is published for programmatic use. |
| Redistribution of data | FBI UCR data is a US federal work, public domain; api.data.gov asks that you not misrepresent the data or imply FBI endorsement. |
| Attribution required? | Cite "FBI Uniform Crime Reporting, Crime Data Explorer" as the source. |
| Rate limits | api.data.gov default is 1,000 requests/hour per key. `DEMO_KEY` returned `x-ratelimit-limit: 10` per hour on 2026-11-23, too little for real use. |

Agency-reported figures are as submitted by each agency; agencies that did not
report a full year carry gaps (population coverage is published beside the
rates in the raw response, which this client does not expose).

## Tests

Offline tests decode synthetic fixtures that keep the CDE layout; no upstream
values are copied. `cargo test -p statsapi-fbi-cde -- --ignored` runs the live
check, which prints `credential_missing: API_DATA_GOV_KEY` and passes when no
key is available.
