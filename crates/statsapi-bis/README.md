# statsapi-bis

`statsapi-bis` reads credit-to-GDP gaps, total credit, residential property prices and debt service ratios from the keyless BIS Data Portal SDMX API.

## Install

```toml
[dependencies]
statsapi-bis = { git = "https://github.com/davidawad/statsapi", package = "statsapi-bis" }
```

## Usage

The same example is in [`examples/basic.rs`](examples/basic.rs):

```rust,no_run
use statsapi_bis::{Client, Series};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let data = client.series(Series::CreditGap, "US", 1)?;
    if let Some(latest) = data.latest() {
        println!("US credit-to-GDP gap {}: {}", latest.period, latest.value);
    }
    Ok(())
}
```

## Authentication

None. The portal needs no key. The client sends a descriptive `User-Agent`, retries 429, 5xx and transport errors with jittered back-off, and writes nothing to disk.

## Endpoints covered

`GET https://stats.bis.org/api/v2/data/dataflow/{agency}/{flow}/{version}/{key}?lastNObservations=N&detail=dataonly`, `Accept: application/vnd.sdmx.data+csv;version=1.0.0`.

| `Series` | Flow | Key (country `C`) | Unit |
| --- | --- | --- | --- |
| `CreditGap` | `BIS/WS_CREDIT_GAP/1.0` | `Q.C.P.A.C` | percentage points of GDP |
| `CreditToGdp` | `BIS/WS_TC/2.0` | `Q.C.P.A.M.770.A` | % of GDP, market value |
| `RealPropertyPriceYoy` | `BIS/WS_SPP/1.0` | `Q.C.R.771` | % year on year, real |
| `DebtServiceRatio` | `BIS/WS_DSR/1.0` | `Q.C.P` | % of income |

`C` is the BIS two-letter code; `bis_country` maps ISO alpha-3 for the economies BIS publishes.

## Terms

| Source | Automated access | Redistribution | Attribution | Rate limits |
| --- | --- | --- | --- | --- |
| BIS Data Portal API | Public, documented SDMX REST API, no key | Statistics may be reproduced with the source cited; see the BIS terms | "Source: BIS" with the dataflow | None published; the client retries politely |

Sources: [BIS Data Portal API](https://data.bis.org/help/tools), [BIS terms and conditions](https://www.bis.org/terms_conditions.htm).

## Tests

Offline tests use a synthesized SDMX-CSV fixture with the portal's column layout. `cargo test -p statsapi-bis -- --ignored` runs the live test.
