# statsapi-fars

`statsapi-fars` downloads and decodes NHTSA Fatality Analysis Reporting System annual CSV archives as a reusable Rust client.

## Install

```toml
[dependencies]
statsapi-fars = { git = "https://github.com/davidawad/statsapi", package = "statsapi-fars" }
```

## Usage

The same example is in [`examples/basic.rs`](examples/basic.rs):

```rust,no_run
use statsapi_fars::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new().with_cache_dir("./fars-cache");
    let data = client.fetch_year(2024)?;
    println!("{} state rows", data.states().len());
    Ok(())
}
```

`Client::new()` uses no cache; `with_cache_dir` opts into a caller-supplied directory. `offline()` reads that cache only and returns an error on a miss. No credentials or API key are required. The client retries transient transport errors and HTTP 429/5xx responses with exponential jitter.

## Endpoints and files

The client fetches the NHTSA national annual archive at `https://static.nhtsa.gov/nhtsa/downloads/FARS/{year}/National/FARS{year}NationalCSV.zip`. It locates and decodes the archive's `accident.csv` and `person.csv` entries, joining fatality counts with measured driver BAC and fatal pedestrian records. It exposes county totals keyed by five-digit FIPS and state totals keyed by two-digit FIPS.

## Terms

Terms reviewed 2026-11-22. The upstream is NHTSA, a U.S. federal agency. Sources: [NHTSA Terms of Use](https://www.nhtsa.gov/about-nhtsa/terms-use) and [FARS dataset README](https://rosap.ntl.bts.gov/view/dot/56262/dot_56262_DS1.pdf).

| Question | Finding |
| --- | --- |
| Automated access allowed? | The terms prohibit attempts to alter information or bypass security. They do not prohibit ordinary automated downloads of the publicly offered annual archive. Use the archive endpoint and retry responsibly. |
| Redistribution of data? | Yes. NHTSA says site information may be distributed or copied; the FARS dataset README identifies the dataset as public domain. |
| Attribution required? | No mandatory attribution found in the cited terms. Cite NHTSA / USDOT as the source as good practice. |
| Rate limits | No numeric limit is stated in the cited terms. The client uses bounded retries for 429 and server errors. |

