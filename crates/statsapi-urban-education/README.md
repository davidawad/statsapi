# statsapi-urban-education

A small blocking Rust client for requesting and decoding paginated Education Data Portal API responses.

## Install

```toml
[dependencies]
statsapi-urban-education = { git = "https://github.com/davidawad/statsapi", package = "statsapi-urban-education" }
```

## Usage

The matching [`examples/basic.rs`](examples/basic.rs) example runs with `cargo run --example basic`.

```rust,no_run
use statsapi_urban_education::{Client, Options, API_ROOT};

let client = Client::with_options(Options {
    cache_dir: Some("./portal-cache".into()),
    offline: false,
    max_retries: 4,
})?;
let url = format!("{API_ROOT}/school-districts/ccd/directory/2024/?fips=34&page_size=5");
let rows = client.rows(&url)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Authentication

The Education Data Portal documentation describes public URL/API requests and does not require credentials. This client sends no key or credential. The upstream is currently keyless.

## Endpoints covered

This client fetches any documented Portal API endpoint and follows its paginated `results` / `next` layout. Commonly used source families and endpoints:

- School and school-district CCD finance, directory, and enrollment.
- School CRDC chronic absenteeism and AP/IB enrollment.
- School and school-district EDFacts grade-eight assessments and graduation rates.
- College/university IPEDS directories and College Scorecard earnings.

The caller supplies the endpoint URL and may supply its own cache directory. No repository configuration or global cache path is consulted by this crate.

## Terms

Terms checked 2026-11-22 against the [Education Data Portal documentation](https://educationdata.urban.org/documentation/) and its [Data Policy and Terms of Use](https://educationdata.urban.org/documentation/).

| Question | Portal terms |
| --- | --- |
| Automated access allowed? | Yes. The documentation describes direct URL endpoint access and API use by software. |
| Redistribution of data | Portal data is made available under the [Open Data Commons Attribution License (ODC-By) 1.0](https://opendatacommons.org/licenses/by/1-0/), which permits redistribution subject to its conditions. |
| Attribution required? | Yes. ODC-By applies; the Portal also provides a recommended citation for research and other published work. |
| Rate limits | No numeric request-rate limit is stated in the documentation or Data Policy and Terms of Use reviewed. |
