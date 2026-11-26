# statsapi-irs-eo

`statsapi-irs-eo` is a blocking downloader and parser for IRS exempt-organization bulk files.

## Install

```toml
[dependencies]
statsapi-irs-eo = { git = "https://github.com/davidawad/statsapi", package = "statsapi-irs-eo" }
```

## Usage

The same flow is compiled from [`examples/basic.rs`](examples/basic.rs).

```rust,no_run
use statsapi_irs_eo::{records, unzip_first, Client, PUB78_URL};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let archive = Client::new()?.download(PUB78_URL)?;
    let (_, rows) = records(&unzip_first(&archive)?)?;
    println!("{} records", rows.len());
    Ok(())
}
```

No key is normally required. The caller controls download, cache, and extract retention; the example keeps the archive and extracted CSV in memory.

## Coverage

| Upstream endpoint or file | Supported access |
| --- | --- |
| IRS Business Master File | Download, ZIP extraction, EIN/name matching, and header-preserving CSV extracts |
| Publication 78 | Download and first-entry extraction |
| Automatic revocation file | Download endpoint constant and ZIP extraction |

## IRS terms

Terms reviewed 2026-11-26: [IRS exempt-organization data page](https://www.irs.gov/charities-non-profits/exempt-organizations-business-master-file-extract-eo-bmf) and [IRS digital products and services terms](https://www.irs.gov/privacy-disclosure/irs-digital-products-and-services-terms-of-use).

| Question | Terms summary |
| --- | --- |
| Automated access allowed? | IRS publishes these bulk extracts for download; automated clients should use the published endpoints and avoid disrupting the service. |
| Redistribution of data | The extracts are public government data, but users remain responsible for privacy, applicable law, and the current IRS terms when republishing. |
| Attribution required? | Identify the IRS exempt-organization file and its release date when presenting or deriving results. |
| Rate limits | No fixed quota is stated for these bulk-file endpoints; use bounded downloads and avoid repeated unnecessary transfers. |
