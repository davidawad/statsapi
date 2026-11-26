# statsapi-census

`statsapi-census` provides blocking clients and parsers for the Census Data API and the keyless Census Reporter ACS mirror.

## Install

```toml
[dependencies]
statsapi-census = { git = "https://github.com/davidawad/statsapi", package = "statsapi-census" }
```

## Usage

The same flow is compiled from [`examples/basic.rs`](examples/basic.rs). Census Reporter needs no key. Set `CENSUS_API_KEY` only when using the Census Data API client.

```rust,no_run
use statsapi_census::Reporter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let reporter = Reporter::new(2023)?;
    let (body, source_url) = reporter.fetch(&["B01003"], &["04000US06"])?;
    println!("{} bytes from {source_url}", body.len());
    Ok(())
}
```

`ApiKey` is optional at the product level: it is required only for `Client` and is supplied by the caller from `CENSUS_API_KEY` in the example.

## Coverage

| Upstream endpoint or file | Supported access |
| --- | --- |
| Census Data API ACS | Caller-built year/dataset/geography queries and array-table parsing |
| Census Reporter ACS 5-year | Keyless table/geography retrieval |
| ACS responses | Stable estimate and margin-of-error maps with missing-value handling |

## Census terms

Terms reviewed 2026-11-26: [Census API Terms of Service](https://www.census.gov/data/developers/about/terms-of-service.html) and [Census Reporter data documentation](https://censusreporter.org/data/).

| Question | Terms summary |
| --- | --- |
| Automated access allowed? | Yes, for services that search, display, analyze, retrieve, or view Census data; the Bureau may limit or block excessive or circumventing use. |
| Redistribution of data | Use and derived services are allowed, but do not combine or publish the data in a way that identifies a person, household, business, or other entity. |
| Attribution required? | Display: “This product uses the Census Bureau Data API but is not endorsed or certified by the Census Bureau,” and do not imply endorsement. |
| Rate limits | The terms reserve access, call, and use limits rather than promising a fixed quota; honor responses and current service guidance. |
