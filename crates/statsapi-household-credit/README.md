# statsapi-household-credit

A reusable Rust client downloads and decodes public household credit datasets from the New York Fed, Federal Reserve Board, and Urban Institute.

## Install

```toml
[dependencies]
statsapi-household-credit = { git = "https://github.com/davidawad/statsapi", package = "statsapi-household-credit" }
```

## Usage

`examples/basic.rs` mirrors this usage and compiles with the crate. Downloads are held in memory only and never written to disk; no global cache or credentials are used.

```rust,no_run
use statsapi_household_credit::{Client, CountyBound, county_debt_to_income_bound};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let archive = client.county_archive()?;
    let interval = county_debt_to_income_bound(&archive, "34023", CountyBound::Low).map_err(std::io::Error::other)?;
    println!("latest interval: {interval:?}");
    Ok(())
}
```

Callers that want a fallback keep a small extract (never the archive) in the `ephemeral-cache` crate; `household-credit-data` does this with a 7-day TTL. The client requires no key; the included endpoints are public. The library has no repository-local dependencies.

## Sources and terms

| Source and endpoint/files covered | Automated access allowed? | Redistribution of data | Attribution required | Rate limits |
|---|---|---|---|---|
| [New York Fed Household Debt and Credit terms](https://www.newyorkfed.org/privacy/termsofuse), quarterly XLSX report | No prohibition found; the terms license website content for personal or business use and distribution. This is an interpretation of the published terms, read 2026-11-22. | Terms permit use, copying, and distribution of content, subject to their conditions. | Yes. Cite “New York Fed Consumer Credit Panel / Equifax” when using CCP data; source-specific attribution takes precedence. | No numeric limit stated in terms, read 2026-11-22. |
| [Urban Institute Debt in America 2026](https://datacatalog.urban.org/dataset/debt-america-2026), county/state/national CSV exports: overall, medical, auto/retail, and student debt | No prohibition found; dataset is public, read 2026-11-22. | ODC-BY license permits reuse and redistribution with attribution; see [ODC-BY 1.0](https://opendatacommons.org/licenses/by/1-0/). | Yes. Dataset asks for citation to Braga, McKernan, and Garon, “Debt in America,” updated November 9, 2026. | No numeric limit stated on dataset page, read 2026-11-22. |
| [Federal Reserve Board county debt ZIP](https://www.federalreserve.gov/releases/z1/dataviz/download/zips/household-debt-by-county.zip), `household-debt-by-county.csv` | No restriction found on the public source endpoint, read 2026-11-22. | No explicit data license stated by the source. The client only parses downloaded data; downstream redistribution should retain source notices and check current terms. | Yes. Cite the Board of Governors of the Federal Reserve System; the source says New York Fed Consumer Credit Panel / Equifax. | No numeric limit found on source page, read 2026-11-22. |

New York Fed site terms do not publish a numeric request limit. Use a reasonable request cadence; transient failures, HTTP 429, and HTTP 5xx receive jittered exponential retries. Terms were checked on 2026-11-22; upstream terms can change.

## API coverage

- `Client::urban_csv` downloads named CSV files from the Urban Debt in America release. `urban_value` reads a selected geographic row and metric column.
- `Client::ny_workbook` downloads the New York Fed quarterly XLSX report. `ny_state_value` reads state debt per capita or 90+ day delinquency.
- `Client::county_archive` downloads the Board of Governors ZIP archive. `county_debt_to_income_bound` reads the latest county low/high interval.
- `Client::fetch` accepts only URLs from the documented upstream hosts and downloads into memory (use `.offline()` to forbid it).

## Verification

- Method: bead-targeted workspace gates plus an isolated standalone `cargo check` of a copied crate with inline workspace lint settings.
- Date: 2026-11-22.
- Results: fmt check 1 pass/0 fail; clippy 1/0; workspace tests 7 passed/0 failed/6 ignored; client ignored live tests 3 passed/0 failed; example build 1/0; isolated standalone check 1/0.
- Fixtures contain synthesized values only; no upstream measurements are stored.
