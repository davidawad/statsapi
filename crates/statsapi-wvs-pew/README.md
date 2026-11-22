# statsapi-wvs-pew

`statsapi-wvs-pew` parses caller-supplied World Values Survey and Pew CSV files without performing network access or retaining survey data.

## Install

```toml
[dependencies]
statsapi-wvs-pew = { git = "https://github.com/davidawad/statsapi", package = "statsapi-wvs-pew" }
```

## Usage

The same example is in [`examples/basic.rs`](examples/basic.rs) and compiles with the crate:

```rust
use statsapi_wvs_pew::{weighted_wvs, WvsMetric};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Supply bytes from a file downloaded and prepared by the caller.
    let csv = b"country,year,weight,generalized_trust\nAAA,2020,2,1\nAAA,2020,1,2\n";
    let result = weighted_wvs(csv, "AAA", WvsMetric::GeneralizedTrust, false)?;
    println!("{result:?}");
    Ok(())
}
```

## Authentication and data access

This crate has no credentials, HTTP client, cache, or filesystem access. Callers obtain files manually through the source portals and pass their bytes to `weighted_wvs` or `pew_aggregate`. WVS downloads require portal registration and acceptance of its conditions of use. Pew dataset downloads use Pew's access process. The ignored smoke tests use `WVS_LOGIN` / `PEW_LOGIN` as presence markers and `WVS_DATA_DIR/wave7.csv` / `PEW_DATA_DIR/aggregates.csv`; without a marker they print `credential_missing` and exit successfully. These environment variables are test conventions, not required by the library.

## Files and measures covered

- WVS Wave 7 files: weighted trust, religion importance, life satisfaction, government confidence, and political interest, using source question columns or documented normalized aliases.
- WVS longitudinal files: the same measures, selecting the latest country survey year present.
- Pew published aggregate CSVs: select a country, metric, and optional demographic group, retaining vintage, field date, and citation URL.

The crate extracts only CSV parsers and weighting logic. It neither downloads nor includes upstream data. Optional cache and credential handling belong to the calling application.

## Terms

Terms reviewed **2026-11-22**. This client does not automate access to either portal and does not bundle survey data.

| Source | Automated access allowed? | Redistribution of data | Attribution required | Rate limits |
| --- | --- | --- | --- | --- |
| World Values Survey | The terms describe registered downloads but do not specify headless retrieval; this crate leaves downloads to the registered user and makes no automated requests. | WVS data files themselves must not be redistributed. Use is limited to non-profit purposes under the download conditions. | Cite the relevant WVS dataset and send citations to WVSA as requested by its conditions. | No numeric rate limit stated in the cited conditions; this parser has no network traffic. |
| Pew Research Center | Unauthorized scraping or automated compilation is prohibited. Dataset access/download is user initiated; this crate makes no automated requests. | Survey data may be distributed only as excerpts, not in full or substantially in full; the license does not cover American Trends Panel datasets. | Cite Pew Research Center and include the required interpretation disclaimer when using data. | No numeric rate limit stated; terms prohibit unreasonable load. This parser has no network traffic. |

Sources: [WVS Wave 7 documentation and conditions of use](https://www.worldvaluessurvey.org/WVSContents.jsp?CMSID=Documentation), [WVS download license](https://www.worldvaluessurvey.org/AJDownloadLicense.jsp), and [Pew Research Center Terms of Use, sections 3 and 13](https://www.pewresearch.org/about/terms-and-conditions/).
