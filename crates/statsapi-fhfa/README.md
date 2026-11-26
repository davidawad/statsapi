# statsapi-fhfa

`statsapi-fhfa` is a blocking client for the FHFA annual house price index (HPI) files: five-digit ZIP, three-digit ZIP, county and census tract.

FHFA has no query API for these indexes, only bulk files. The client therefore follows the source order live API, then streamed download with on-the-fly filtering. It downloads the file for one level, keeps only the rows of the requested geography, and persists nothing: no cache, no temp file. The XLSX files (ZIP5 about 40 MB, county 5 MB, ZIP3 2 MB) are buffered in memory only for the call; the tract CSV (about 90 MB) is read as a stream and reading stops after the tract's block.

## Install

```toml
[dependencies]
statsapi-fhfa = { git = "https://github.com/davidawad/statsapi", package = "statsapi-fhfa" }
```

## Usage

See [`examples/basic.rs`](examples/basic.rs).

```rust,no_run
use statsapi_fhfa::{Client, Error, Level};

fn main() -> Result<(), Error> {
    let client = Client::new()?;
    if let Some(latest) = client.series(Level::County, "34023")?.latest() {
        println!("HPI {} in {}", latest.hpi, latest.year);
    }
    Ok(())
}
```

`Series::latest` returns the newest year's index, FHFA's one-year change, and 5- and 10-year cumulative changes computed from index values. Each `Client::series` call downloads the whole level file once; callers asking several metrics of one geography should reuse the returned `Series`.

## Authentication

None. The files are public downloads.

## Files covered

| Level | File | Key |
| --- | --- | --- |
| `Level::Zip5` | [`hpi_at_zip5.xlsx`](https://www.fhfa.gov/hpi/download/annual/hpi_at_zip5.xlsx) | five-digit ZIP, leading zeros kept |
| `Level::Zip3` | [`hpi_at_zip3.xlsx`](https://www.fhfa.gov/hpi/download/annual/hpi_at_zip3.xlsx) | first three ZIP digits |
| `Level::County` | [`hpi_at_county.xlsx`](https://www.fhfa.gov/hpi/download/annual/hpi_at_county.xlsx) | five-digit county FIPS |
| `Level::Tract` | [`hpi_at_tract.csv`](https://www.fhfa.gov/hpi/download/annual/hpi_at_tract.csv) | eleven-digit tract FIPS |

FHFA labels the ZIP3, ZIP5 and county indexes developmental. A missing value is published as a period and is skipped. The `.csv` URL for ZIP5 serves an XLSX workbook; the client uses the `.xlsx` names.

## Terms

Reviewed 2026-11-23 at [fhfa.gov/data/hpi/datasets](https://www.fhfa.gov/data/hpi/datasets) and [robots.txt](https://www.fhfa.gov/robots.txt).

| Question | Finding | Source |
| --- | --- | --- |
| Automated access allowed? | No prohibition stated for the `/hpi/download/` files; robots.txt does not disallow them. The client sends a descriptive user agent and retries only 429/5xx/timeouts with jittered backoff. | datasets page, robots.txt |
| Redistribution of data | FHFA is a federal agency and the HPI is published for public use; this crate redistributes no upstream data (fixtures are synthetic). | datasets page |
| Attribution required? | None stated; "Source: FHFA House Price Index" is given as a courtesy. | datasets page |
| Rate limits | None stated. One download per call, no polling. | datasets page |

## Verified

Fixtures are synthetic in the upstream layouts. Live check 2026-11-23: county 34023 (Middlesex, NJ), HPI 1195.35 for 2025 from `hpi_at_county.xlsx`; `cargo test -p statsapi-fhfa -- --ignored` passes.

## License

GPL-3.0-or-later.
