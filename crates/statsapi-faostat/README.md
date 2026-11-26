# statsapi-faostat

`statsapi-faostat` is a reusable blocking Rust client for FAOSTAT. It streams a
public FAOSTAT bulk "All Data (Normalized)" zip into memory, filters one
country and the requested item/element pairs while reading, and persists
nothing (no cache, no archive on disk).

## Why bulk files

FAOSTAT's JSON API (`faostatservices.fao.org/api/v1`) answers `Missing
Authorization Header` without a login, and the older
`fenixservices.fao.org/faostat/api/v1` endpoint returned HTTP 521 on
2026-11-23. The bulk files at `bulks-faostat.fao.org/production` are keyless,
so the client uses them (source order 2: streamed download plus filter).

## Usage

```rust
use statsapi_faostat::{latest, Dataset, FaostatClient, Selector};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let selector = Selector { item: "210041".into(), element: "6121".into() };
    let (url, rows) = FaostatClient::new()?.fetch(
        Dataset::FoodSecurity, "USA", std::slice::from_ref(&selector))?;
    println!("{url}");
    if let Some(row) = latest(&rows, &selector) {
        println!("{} {} ({})", row.value, row.unit, row.vintage());
    }
    Ok(())
}
```

Domains: `QCL` crops and livestock (Production_Crops_Livestock, ~34 MB),
`FS` food security (~2 MB), `CP` consumer price indices (~2 MB). Countries are
ISO 3166-1 alpha-3, mapped to UN M49 by a built-in table. The in-memory
download is capped at 128 MiB. Censored values such as `<2.5` come back as
the bound with `qualifier = Some('<')`. Monthly series carry `month` and the
`vintage()` is `YYYY-MM`.

## Terms

Checked 2026-11-23 against the [FAO terms of use for statistical
databases](https://www.fao.org/contact-us/terms/db-terms-of-use/en/).

| Question | Finding |
| --- | --- |
| Automated access allowed? | The terms do not prohibit automated download; the bulk files are public and keyless. The prohibited-uses clause covers misrepresenting FAO and commercial resale of the database itself. |
| Redistribution of data | CC BY 4.0 with the FAO additional terms; derived values may be redistributed with attribution. |
| Attribution required? | Yes: "FAO. [year of last update]. [dataset name]. FAOSTAT. https://www.fao.org/faostat". |
| Rate limits | None stated. One request per dataset per run; nothing is cached, so avoid tight loops. |

The crate bundles no upstream data (the test fixture is synthetic).
