# statsapi-vdem

`statsapi-vdem` is a blocking client and projection parser for the V-Dem Country-Year Core dataset.

## Install

```toml
[dependencies]
statsapi-vdem = { git = "https://github.com/davidawad/statsapi", package = "statsapi-vdem" }
```

## Usage

The same flow is compiled from [`examples/basic.rs`](examples/basic.rs).

```rust,no_run
use statsapi_vdem::{latest, Client};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let csv = Client::new()?.fetch_extract()?;
    println!("{:?}", latest(&csv, "USA", "v2x_libdem")?);
    Ok(())
}
```

Authentication is not required. The caller owns archive and extract retention.

## Coverage

| Upstream endpoint or file | Supported access |
| --- | --- |
| V-Dem Country-Year Core CSV ZIP | Download and project the country ID, year, and five supported indices |
| Country-year observations | Return the latest numeric value for a country and selected projected column |

## V-Dem terms

Terms reviewed 2026-11-26: [V-Dem dataset page](https://www.v-dem.net/data/the-v-dem-dataset/), [V-Dem FAQ](https://v-dem.net/about/faq/), and [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/).

| Question | Terms summary |
| --- | --- |
| Automated access allowed? | The dataset is publicly downloadable; automated retrieval should use the published archive and respect service availability. |
| Redistribution of data | Use, adaptation, and sharing are allowed under CC BY-SA 4.0, including commercial use; derivatives must retain the share-alike terms. |
| Attribution required? | Credit the V-Dem Project, cite the dataset version, and use the project’s suggested citation. |
| Rate limits | No fixed API quota is stated for the archive; avoid unnecessary repeated downloads and retain the version used. |
