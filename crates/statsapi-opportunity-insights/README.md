# statsapi-opportunity-insights

`statsapi-opportunity-insights` downloads and decodes Opportunity Insights public CSV datasets for Rust applications.

## Install

```toml
[dependencies]
statsapi-opportunity-insights = { git = "https://github.com/davidawad/statsapi", package = "statsapi-opportunity-insights" }
```

## Usage

The same example is in [`examples/basic.rs`](examples/basic.rs):

```rust,no_run
use statsapi_opportunity_insights::{Client, Dataset};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let table = client.fetch(Dataset::SocialCapitalZip)?;
    println!("{} records", table.rows().len());
    Ok(())
}
```

## Authentication and cache

These public file downloads do not require an API key. The crate does not read credentials or environment variables. Set a cache directory explicitly with `Client::builder().cache_dir(path)`; call `.offline(true)` to require a cache hit and avoid network requests. The default client has no cache.

## Endpoints and files

The client downloads and decodes these CSV files:

- Opportunity Atlas tract outcomes: `tract_outcomes_simple.csv`.
- Social Capital Atlas: `social_capital_zip.csv` and `social_capital_county.csv` from the Humanitarian Data Exchange.
- Opportunity Insights Economic Tracker: Affinity consumer spending (monthly), Womply small business revenue (weekly), and employment (weekly), each by county or state.

## Terms

Terms were reviewed on **2026-11-22**. The [Opportunity Insights Data Library](https://opportunityinsights.org/data/) describes its datasets as publicly available for researchers and practitioners and links the Social Capital Atlas downloads and license. Its [Social Capital II listing](https://opportunityinsights.org/data/?geographic_level=0&paper_id=1652&topic=0) links the Humanitarian Data Exchange license and README. The data page does not publish a general automated-download rate limit or a blanket redistribution grant; follow each linked dataset license and cite the source. The license PDF linked from HDX was not retrievable during this review, so its exact redistribution conditions are not restated here.

| Question | Published terms observed |
| --- | --- |
| Automated access allowed? | Public downloadable CSVs are listed; no automation restriction is stated on the OI data page. No request quota is published there. |
| Redistribution of data | Dataset-specific. Social Capital files link to an HDX license; the data page does not grant blanket redistribution rights. Check the source license before redistribution. |
| Attribution required? | Cite the relevant Opportunity Insights research/data source; the data page links dataset READMEs and citations. |
| Rate limits | None stated on the OI data page or in the linked data-library listing reviewed. Use a cache and avoid excessive requests. |

Sources: [Opportunity Insights Data Library](https://opportunityinsights.org/data/), [Social Capital II listing and linked license](https://opportunityinsights.org/data/?geographic_level=0&paper_id=1652&topic=0), [Economic Tracker](https://economictracker.org/).
