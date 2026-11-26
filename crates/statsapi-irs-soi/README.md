# statsapi-irs-soi

`statsapi-irs-soi` fetches and decodes reusable IRS Statistics of Income ZIP income and county migration CSV data.

## Install

```toml
[dependencies]
statsapi-irs-soi = { git = "https://github.com/davidawad/statsapi", package = "statsapi-irs-soi" }
```

## Usage

See [`examples/basic.rs`](examples/basic.rs) for a compiling complete example. The client is blocking, uses rustls, retries transient network errors and HTTP 429/5xx with jittered backoff, and streams each bulk CSV and keeps only the requested place's rows (the file is never written to disk and no archive is kept). The IRS has no query API, so the path is `stream`; `Client::with_cache_dir(path)` adds a last-resort ephemeral cache (`ephemeral-cache` crate) of those small per-place extracts: TTL 7 days, size cap 8 MiB, oldest evicted first. The cache answers (`cache`) only when the stream fails. Chain `.offline()` to prohibit network access and require a fresh cache hit. `zip_metric` and `migration_metric` return the value with the `AnswerPath` that answered.

```rust,no_run
use statsapi_irs_soi::{Client, Error};

fn main() -> Result<(), Error> {
    let cache = std::env::var_os("IRS_SOI_DATA_CACHE_DIR").map(std::path::PathBuf::from);
    let mut client = cache.map_or_else(Client::new, Client::with_cache_dir);
    if let Some((returns, path)) = client.zip_metric("10001", "returns")? {
        println!("ZIP 10001 reported {returns} returns ({})", path.as_str());
    }
    Ok(())
}
```

## Authentication

No credential or API key is needed. The IRS datasets are public downloads.

## Endpoints and files covered

| Dataset | Source file | Coverage exposed |
| --- | --- | --- |
| Individual income ZIP data | [`23zpallagi.csv`](https://www.irs.gov/pub/irs-soi/23zpallagi.csv) | ZIP returns, AGI, mean AGI, wages, dividends, capital gains, EITC share, AGI $200k+ share, itemizer share |
| County migration inflow | [`countyinflow2324.csv`](https://www.irs.gov/pub/irs-soi/countyinflow2324.csv) | County or state inflow returns and AGI |
| County migration outflow | [`countyoutflow2324.csv`](https://www.irs.gov/pub/irs-soi/countyoutflow2324.csv) | County or state outflow returns and AGI |

ZIP metrics: `returns`, `agi`, `mean_agi`, `wages`, `dividends`, `capital_gains`, `eitc_share`, `agi_200k_share`, and `itemizers_share`. Migration metrics: `in_migration_returns`, `out_migration_returns`, `net_migration_returns`, `in_migration_agi`, `out_migration_agi`, and `net_migration_agi`. County IDs are five digit state-plus-county FIPS strings; state IDs are two digit FIPS strings.

## Terms

Terms reviewed 2026-11-22. The [IRS privacy policy](https://www.irs.gov/privacy-disclosure/irs-privacy-policy) discusses personal information provided by website visitors; this client requests published aggregate CSV files without sending personal information. SOI pages describe downloadable ZIP and migration datasets.

| Question | Finding | Source |
| --- | --- | --- |
| Automated access allowed? | No prohibition on automated downloads is stated on the reviewed pages. This client uses a conservative retry policy for 429 and server errors. | [IRS privacy policy](https://www.irs.gov/privacy-disclosure/irs-privacy-policy), [ZIP data](https://www.irs.gov/statistics/soi-tax-stats-individual-income-tax-statistics-zip-code-data-soi), [migration data](https://www.irs.gov/statistics/soi-tax-stats-migration-data) |
| Redistribution of data | The crate redistributes no upstream data. IRS data are federal government works; downstream users should consult applicable law and current source terms before redistributing datasets. | [IRS privacy policy](https://www.irs.gov/privacy-disclosure/irs-privacy-policy), SOI pages above |
| Attribution required? | The reviewed pages state no attribution requirement for these downloads; attribution to IRS SOI is provided as a courtesy. | SOI pages above |
| Rate limits | The reviewed pages state no numeric request limit. Use reasonable request rates; transient 429/5xx responses receive jittered backoff. | SOI pages above |


## License

GPL-3.0-or-later.
