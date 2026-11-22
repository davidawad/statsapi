# statsapi-sdmx

`statsapi-sdmx` fetches and decodes caller-selected observations from public SDMX APIs.

## Install

```toml
[dependencies]
statsapi-sdmx = { git = "https://github.com/davidawad/statsapi", package = "statsapi-sdmx" }
```

## Usage

The same example is in [`examples/basic.rs`](examples/basic.rs):

```rust,no_run
use statsapi_sdmx::{Agency, Client};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = statsapi_sdmx::data_url(
        Agency::Ecb,
        None,
        "ICP",
        "M.FR.N.000000.4.ANR",
        None,
    );
    let client = Client::builder()
        .cache_dir(std::env::temp_dir().join("statsapi-sdmx-cache"))
        .build()?;
    let response = client.get(&url)?;
    let (value, period) = statsapi_sdmx::parse_observation(&response)?;
    println!("ECB observation for {period}: {value}");
    Ok(())
}
```

## Authentication

The listed public statistical endpoints do not require credentials. The crate
does not read secrets from the environment. If an upstream changes its public
access policy, authentication support should take a caller-provided key or a
documented agency-specific environment variable; never commit a key.

## Endpoints and formats

The client builds focused data requests to these public endpoints. The caller
provides the dataflow and dimension key; agency conventions choose the request
path, default response format, and query options.

| Agency | Default base | Request path / default format |
| --- | --- | --- |
| OECD | `https://sdmx.oecd.org/public/rest` | `/data/{flow}/{key}` with one last observation; CSV by default |
| Eurostat | `https://ec.europa.eu/eurostat/api/dissemination/sdmx/2.1` | `/data/{flow}/{key}?format=JSON&lang=en` |
| IMF | `https://www.imf.org/external/datamapper/api/v1` | `/{flow}/{key}?format=jsondata`; no bulk-download helper |
| ILO | `https://rplumber.ilo.org/data/indicator` | `/{flow}/{key}?format=jsondata` |
| UNESCO UIS | `https://api.uis.unesco.org/api/public` | `/{flow}/{key}?format=jsondata` |
| ECB | `https://data-api.ecb.europa.eu/service` | `/data/{flow}/{key}?format=csvdata&lastNObservations=1` |

`parse_observation` decodes common SDMX JSON observations, Eurostat JSON-stat,
and SDMX CSV with `OBS_VALUE` and optional `TIME_PERIOD` columns. This is a
focused observation client, not a bulk exporter or a complete SDMX 2.1/3.0
implementation. IMF terms restrict systematic downloads and require permission
for commercial reuse; honor the terms linked below.

## Terms reviewed 2026-11-22

These summaries describe the linked agency pages read on the date above.
Dataset-specific and third-party restrictions can apply. Limits marked
“unspecified” were not stated in the terms or API guide reviewed; follow any
server response or updated guidance.

| Agency and terms | Automated access allowed? | Data redistribution | Attribution | Rate limits |
| --- | --- | --- | --- | --- |
| [OECD terms and conditions](https://www.oecd.org/en/about/terms-conditions.html), [API guidance](https://www.oecd.org/en/data/insights/data-explainers/2024/11/Api-best-practices-and-recommendations.html) | Yes, OECD APIs are offered for data access. Do not modify or redistribute the API itself. | Data may generally be copied, adapted, shared, and redistributed, subject to dataset and third-party restrictions. | Cite the dataset citation or OECD, dataset/source, URL/DOI, and access date. | 60 data downloads/hour; respect query restrictions and server controls. |
| [Eurostat API access](https://ec.europa.eu/eurostat/web/user-guides/data-browser/api-data-access/api-introduction), [reuse policy](https://ec.europa.eu/eurostat/help/copyright-notice) | Yes; the API guide identifies this interface for automated queries and downloads. | Generally allowed, including commercial reuse, except listed third-party and dataset-specific exceptions. | Acknowledge Eurostat/source and dataset, and indicate changes. | Unspecified in the pages reviewed. |
| [IMF API guide](https://data.imf.org/en/Resource-Pages/IMF-API), [copyright and usage](https://www.imf.org/en/About/copyright-and-terms) | Official SDMX APIs support importing datasets into applications. IMF general terms require permission for systematic downloads; this client does not provide bulk downloads. | Published IMF statistical data may be redistributed with conditions; systematic downloads require written permission and commercial reuse requires permission. | Attribute IMF and the dataset link; state material transformations. | Unspecified in the pages reviewed; avoid systematic/bulk use without permission. |
| [ILO rights and permissions](https://www.ilo.org/rights-and-permissions) | Public data/API use is not prohibited by the reviewed rights page. | ILO knowledge products and datasets released since 2023-05-03 are generally CC BY 4.0; exceptions and third-party rights apply. | Cite ILO as source; credit original author where specified. | Unspecified in the page reviewed. |
| [UNESCO UIS terms](https://databrowser.uis.unesco.org/terms-and-conditions) | UIS encourages sharing and reproduction; no API-specific automated limit was stated. | Generally shareable and reproducible under CC BY-SA 4.0, unless material says otherwise. | Cite UIS, complete URL, and extraction date. | Unspecified in the terms reviewed. |
| [ECB/ESCB reuse policy](https://www.ecb.europa.eu/stats/ecb_statistics/governance_and_quality_framework/html/usage_policy.en.html), [SDMX API overview](https://data.ecb.europa.eu/help/api/overview) | Yes; the SDMX service provides programmatic access to statistical data and metadata. | Public statistics may be reused free of charge; do not modify statistics/metadata, and check NCB/third-party disclaimers. | Quote the source, such as “Source: ECB statistics.” | Unspecified in the pages reviewed; exceptional access restrictions may apply. |

## Tests

Offline fixture tests cover the supported CSV and JSON layouts. Fixtures retain
the response shape but contain synthesized values and periods, not upstream
observations. Live tests are ignored by default:

```text
cargo test -p statsapi-sdmx -- --ignored --nocapture
```
