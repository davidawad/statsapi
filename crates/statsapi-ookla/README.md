# statsapi-ookla

`statsapi-ookla` is a blocking downloader and parser for Ookla Open Data performance parquet releases.

## Install

```toml
[dependencies]
statsapi-ookla = { git = "https://github.com/davidawad/statsapi", package = "statsapi-ookla" }
```

## Usage

The same flow is compiled from [`examples/basic.rs`](examples/basic.rs). Set `OOKLA_PARQUET_URL` to a performance parquet object URL before running it; `OOKLA_PARQUET_PATH` can select the temporary extract path.

```rust,no_run
use std::{env, path::PathBuf};
use statsapi_ookla::{aggregate_reader, download, prefixes, value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(env::var("OOKLA_PARQUET_PATH")?);
    let response = download(&env::var("OOKLA_PARQUET_URL")?)?;
    let totals = aggregate_reader(response, &prefixes(34.05, -118.25), &path)?;
    println!("{:?} Mbps", value(&totals, "download_mbps"));
    Ok(())
}
```

Authentication is not required. The caller supplies the release URL, scratch path, and any retention policy.

## Coverage

| Upstream endpoint or file | Supported access |
| --- | --- |
| Ookla performance fixed/mobile parquet | Download and stream a release into a caller-owned path |
| Zoom-12 quadkeys | Generate the nine tile prefixes around a latitude/longitude |
| Tile metrics | Test-weighted download, upload, latency, test, and device totals |

## Ookla terms

Terms reviewed 2026-11-26: [Ookla Open Data README and dataset notes](https://github.com/teamookla/ookla-open-data/blob/master/README.md).

| Question | Terms summary |
| --- | --- |
| Automated access allowed? | The open-data repository publishes downloadable parquet and shapefile releases for programmatic use. |
| Redistribution of data | Releases are under CC BY-NC-SA 4.0: noncommercial redistribution and adaptations must preserve the license and share alike. |
| Attribution required? | Credit Ookla and Speedtest, link the source, and identify changes where applicable. |
| Rate limits | The README publishes release cadence rather than an API quota; callers should avoid abusive download concurrency and cache responsibly. |
