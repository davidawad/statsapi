# statsapi-cms

A small blocking Rust client for the keyless CMS `data.cms.gov` data-api v1, covering Medicare Geographic Variation and Part D opioid prescribing rates by geography.

## Install

```toml
[dependencies]
statsapi-cms = { git = "https://github.com/davidawad/statsapi", package = "statsapi-cms" }
```

## Usage

See [`examples/basic.rs`](examples/basic.rs).

```rust,no_run
use statsapi_cms::{Client, Level};

let client = Client::default();
let row = client.geographic_variation(Level::County, "34023")?;
if let Some(row) = row {
    println!("{:?}", row.number("TOT_MDCR_PYMT_PC"));
}
# Ok::<(), statsapi_cms::Error>(())
```

## Auth

None. The API is keyless. There is no cache: each call is one live request, retried with jitter on 429, 5xx and transport errors.

## Endpoints covered

| Call | Dataset | Filters |
|---|---|---|
| `geographic_variation(level, code)` | Medicare Geographic Variation, by National, State & County (`6219697b-8f6c-4164-bed4-cd9317c58ebc`) | `BENE_GEO_LVL`, `BENE_GEO_CD`, `BENE_AGE_LVL=All`, newest `YEAR` |
| `opioid_prescribing(level, code)` | Medicare Part D Opioid Prescribing Rates, by Geography (`94d00f36-73ce-4520-9b3f-83cd3cded25c`) | `Prscrbr_Geo_Lvl`, `Prscrbr_Geo_Cd`, `Breakout_Type=Totals`, newest `Year` |
| `rows(&Query)` | any data-api dataset | caller-supplied `filter[...]`, `sort[...]`, `column`, `size` |

County codes are 5-digit FIPS; state codes are 2-digit FIPS.

## Terms

| Item | Position | Source |
|---|---|---|
| Automated access | Permitted; the data-api is a public, keyless API | <https://data.cms.gov/> |
| Redistribution | CMS data are U.S. federal public data; CMS asks that data not be altered while implying CMS endorsement | <https://www.cms.gov/about-cms/agency-information/aboutwebsite/cms-web-policies> |
| Attribution | Cite Centers for Medicare & Medicaid Services, dataset name and year | <https://data.cms.gov/> |
| Rate limits | None published; the client retries 429 and 5xx with jitter | <https://data.cms.gov/> |

Tests use a synthetic fixture with the upstream layout. `cargo test -p statsapi-cms -- --ignored` runs one live check.
