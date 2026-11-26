# statsapi-seda

`statsapi-seda` is a blocking Rust client for downloading and decoding selected Stanford Education Data Archive CSV rows.

## Install

```toml
[dependencies]
statsapi-seda = { git = "https://github.com/davidawad/statsapi", package = "statsapi-seda" }
```

## Usage

```rust,no_run
use statsapi_seda::{Client, Dataset, Level};

fn main() -> Result<(), statsapi_seda::Error> {
    let client = Client::new()?;
    let table = client.fetch(Level::County, Dataset::Achievement, "00001")?;
    println!("{} rows from {}", table.rows.len(), table.source_url);
    Ok(())
}
```

The same call is in [`examples/basic.rs`](examples/basic.rs). The geography code in that example is synthetic; use an identifier from a release you are authorized to access.

## Authentication and access

The supported CSV file URLs do not require an API key. Users must review and accept Stanford's data use agreement before accessing the files. The client does not collect credentials or accept an authentication token. The release CSVs are streamed and filtered to the requested geography as they download; nothing is persisted (no cache, no archive), and `Client::trail()` reports the `stream` access path.

## Files covered

`Client::fetch` selects the CSV source from the requested geography and dataset:

- District and state achievement (`cs`) and growth (`gcs`) annual subject files from SEDA 2024.1.
- County achievement (`cs`) and growth (`gcs`) pooled subject files from SEDA 6.0.

Each result retains the CSV headers, rows matching the requested geography, source URL, and release vintage. Requests use a 280-second timeout and jittered retries for HTTP 429, server errors, and transport failures.

## Stanford data use terms

Source: [SEDA Get the Data and Data Use Agreement](https://edopportunity.org/trends/data/), read 2026-11-22.

| Question | Terms summary |
| --- | --- |
| Automated access allowed? | The agreement does not state a prohibition on automated access. It grants access and use subject to the agreement. |
| Redistribution of data | Do not publish data files, in full or in part, without explicit permission. The agreement also bars commercial advantage and for-profit use. |
| Attribution required? | Yes. Acknowledge SEDA as the source and use the dataset citation specified by Stanford in publications. |
| Rate limits | The agreement states no request rate limit. Use reasonable request rates and comply with any applicable Stanford terms. |

The table summarizes the published agreement; it is not a grant to redistribute SEDA data. No upstream data values are included in this crate's fixtures.

## Verification

Verified on 2026-11-22. Method: workspace `cargo fmt`, clippy, tests, ignored live tests, and example build, plus `cargo check` after copying the crate into an isolated workspace with the workspace lints inlined. The ordinary test run passed 10 tests, failed 0, and ignored 3 live tests; the ignored run passed all 3 live tests, failed 0. Formatting, clippy, example build, and standalone check each passed. Offline fixture checks use synthesized values and a SEDA-compatible county CSV layout; this verification records no upstream values.
