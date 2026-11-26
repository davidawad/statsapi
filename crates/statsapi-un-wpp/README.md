# statsapi-un-wpp

Reusable blocking Rust client for the UN World Population Prospects (WPP) 2024 CSV downloads. No repo dependencies.

## Usage

```rust
use statsapi_un_wpp::{WppClient, LAST_ESTIMATE_YEAR};

let client = WppClient::new()?;
if let Some(rows) = client.fetch_country("USA")? {
    println!("{:?}", rows.latest_value("MedianAgePop", LAST_ESTIMATE_YEAR));
    println!("{:?}", rows.value(2050, "MedianAgePop"));
}
```

See [`examples/basic.rs`](examples/basic.rs). `parse_csv` parses any plain-text WPP indicators CSV (used by the fixture tests).

## Source access

The UN Population Division Data Portal API (`https://population.un.org/dataportalapi/api/v1/`) lists indicators and locations without a key but answers `401 Bearer` for every `/data/` call, so it is not used (keyless sources only). The client instead streams `WPP2024_Demographic_Indicators_Medium.csv.gz` (about 16 MB), decodes gzip on the fly, keeps only the requested country's rows (about 150 years, one row per year) and writes nothing to disk. No cache and no bulk archive are kept. Estimates run to 2023; later years are medium-variant projections.

Columns served by the adapter: `TPopulation1July` (thousands), `MedianAgePop`, `TFR`, `LEx`, `NetMigrations` (thousands). Dependency ratios exist only in the token-gated portal API (indicators 83, 84, 86) and are not in this file, so they are not served.

## Terms

| Item | Value |
| --- | --- |
| Publisher | UN DESA, Population Division |
| Terms | <https://population.un.org/wpp/> (data licensed CC BY 3.0 IGO; cite "United Nations, DESA, Population Division. World Population Prospects 2024") |
| Key | none for the CSV downloads; the Data Portal API data endpoints need a bearer token |
| Automated access | CSV downloads are public static files; one request per adapter call, 150 s timeout |
| Client license | GPL-3.0-or-later, `publish = false` |
