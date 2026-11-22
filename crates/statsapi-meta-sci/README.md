# statsapi-meta-sci

A blocking Rust client downloads and decodes Meta Social Connectedness Index files published on HDX.

## Install

```toml
[dependencies]
statsapi-meta-sci = { git = "https://github.com/davidawad/statsapi", package = "statsapi-meta-sci" }
```

## Usage

The same example is in [`examples/basic.rs`](examples/basic.rs).

```rust,no_run
use statsapi_meta_sci::{Client, Source};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new().with_cache_dir("./sci-cache");
    let rows = client.rows(Source::County, "06037")?;
    for row in rows.iter().take(5) {
        println!("{}: {}", row.friend, row.score);
    }
    Ok(())
}
```

Authentication is not required for the published HDX/Google Drive download files. The caller may provide a cache directory for persistent filtered extracts; without one, network reads still work and are not cached. `Client::offline(true)` limits reads to existing cached extracts. No environment variables or repository configuration are read by this crate.

## Files covered

| Source | File | Data |
| --- | --- | --- |
| `Source::Country` | [country.csv](https://data.humdata.org/dataset/e9988552-74e4-4ff4-943f-c782ac8bca87/resource/652cf9c9-541f-47de-8d53-ff818062bd0c/download/country.csv) | Country-to-country SCI |
| `Source::County` | [us_counties.csv](https://data.humdata.org/dataset/e9988552-74e4-4ff4-943f-c782ac8bca87/resource/97dc352f-c9c5-47d6-a6ef-88709e14006c/download/us_counties.csv) | US county-to-county SCI |
| `Source::Zcta` | [US ZCTA archive](https://drive.google.com/uc?export=download&id=13gbCdgHD-xfkogDoSzcAGKhNLHMPy8tR) | US ZCTA-to-ZCTA SCI, selecting the relevant first-digit shard |
| `Client::county_centroids` | [2025 Census Gazetteer](https://www2.census.gov/geo/docs/maps-data/data/gazetteer/2025_Gazetteer/2025_Gaz_counties_national.zip) | US county centroid coordinates (US federal public domain) |

The client filters CSV files to the requested origin, parses `scaled_sci`, and caches only the filtered rows when a directory is supplied. HTTP transport errors and upstream 429/5xx responses receive bounded retries with jitter.

## Terms reviewed 2026-11-22

| Question | Finding |
| --- | --- |
| Automated access allowed? | HDX publishes direct downloadable files and the dataset metadata lists no API key or rate limit. Downloads are from HDX/Google Drive, not Meta products. Meta's [Automated Data Collection Terms](https://www.facebook.com/legal/automated_data_collection_terms) govern automated collection from Meta products; they do not prohibit this client's access to HDX-hosted files. |
| Redistribution of data | HDX lists the SCI dataset as “Public Domain / No restrictions (CC0).” See the [HDX dataset](https://data.humdata.org/dataset/social-connectedness-index) and [HDX license information](https://data.humdata.org/about/license). This client does not redistribute the data. |
| Attribution required? | CC0 does not require attribution. HDX includes recommended academic citations in its dataset notes; cite Bailey et al. (2018) and Johnston et al. (2026) when publishing results. Meta's [SCI page](https://ai.meta.com/ai-for-good/datasets/social-connectedness-index/) identifies the dataset and links to HDX. |
| Rate limits | The HDX dataset metadata states no rate limit. This client retries 429 and 5xx responses with bounded jitter and callers should avoid unnecessary downloads by supplying a cache directory. Meta's [platform rate limits](https://developers.facebook.com/docs/graph-api/overview/rate-limiting/) concern Meta APIs and do not apply to HDX file downloads. |
