# statsapi-cdc-wonder

`statsapi-cdc-wonder` is a reusable blocking Rust client for CDC WONDER national mortality tables and the NCHS state and county mortality datasets on data.cdc.gov.

## Install

```toml
[dependencies]
statsapi-cdc-wonder = { git = "https://github.com/davidawad/statsapi", package = "statsapi-cdc-wonder" }
```

## Usage

The same flow is in [`examples/basic.rs`](examples/basic.rs):

```rust
use statsapi_cdc_wonder::{latest_row, Cause, WonderClient};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = WonderClient::new()?.with_cache_dir("./cdc-cache");
    let national = client.national_series(Cause::DrugOverdose)?;
    if let Some(row) = latest_row(&national.value) {
        println!("{} {:?} ({})", row.year, row.age_adjusted_rate, national.path.as_str());
    }
    let county = client.drug_poisoning_county("34023")?;
    println!("{} {}", county.value.year, county.value.rate);
    Ok(())
}
```

No API key is needed and none is read. Every result is a `Fetched<T>` with the
exact URL and the access path (`api` or `cache`). Access order: the live API
first; the optional cache directory is read only when the live request fails
or in `offline()` mode. Cached entries expire after 7 days and the directory
is held under 4 MiB. Retries use jitter on HTTP 429, 5xx and transport errors.

## Coverage

- **CDC WONDER API**, database D76 (Underlying Cause of Death, 1999-2020,
  final), POST `https://wonder.cdc.gov/controller/datarequest/D76` with a
  `request_xml` form field. Causes: suicide (X60-X84), drug overdose (drug
  induced D1-D4), alcohol induced (A9), cancer (C00-C97), grouped by year, with
  age-adjusted rate, 95% interval and standard error. **National only**: CDC
  states that the API cannot group by or filter on region, state, county or
  urbanization, so sub-national values are unavailable through the API.
- **data.cdc.gov (Socrata SODA)**: `bi63-dtpu` leading causes by state (to
  2017), `xbxb-epbu` drug poisoning by state (to 2016), `rpvx-m2md` drug
  poisoning by county (model-based, to 2021).

WONDER requires at least 15 seconds between requests; the client spaces its
own requests and retries once another process has used the window. wonder.cdc.gov's
edge returns 403 unless the user agent is browser-shaped, so the client sends
`Mozilla/5.0 (compatible; statsapi-cdc-wonder/<version>; +<repo url>)`.

## Terms

Checked 2026-11-23 against the [WONDER API page](https://wonder.cdc.gov/wonder/help/wonder-api.html),
the [WONDER data use restrictions](https://wonder.cdc.gov/DataUse.html) and the
[data.cdc.gov terms](https://www.cdc.gov/other/agencyfaqs/index.html).

| Question | Finding |
| --- | --- |
| Automated access allowed? | Yes for WONDER: the API exists for automated XML queries, each request must carry `accept_datause_restrictions=true`. data.cdc.gov is open SODA access. |
| Redistribution of data | US government work, public domain. WONDER restrictions: no attempt to identify individuals; counts under 10 are suppressed and must not be re-derived. |
| Attribution required? | Citing CDC/NCHS and the dataset is expected. |
| Rate limits | WONDER: at least 15 seconds between requests, errors otherwise. data.cdc.gov: unauthenticated requests are throttled by IP; no numeric limit published. |

The client bundles no upstream data. Offline fixtures use synthetic values only.
