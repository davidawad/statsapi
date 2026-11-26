# statsapi-owid

`statsapi-owid` is a reusable blocking Rust client for Our World in Data
(OWID) grapher CSV endpoints. It has no dependencies on this repository
(`reqwest` blocking + rustls, `csv`).

```rust
use statsapi_owid::OwidClient;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let observation = OwidClient::new()?.latest("life-expectancy", "USA")?;
    println!("{} {}", observation.year, observation.value);
    Ok(())
}
```

`latest(slug, iso3)` requests
`https://ourworldindata.org/grapher/<slug>.csv?csvType=filtered&country=~<ISO3>`
and filters the CSV to the country while it streams, returning the newest
numeric value of the first data column with the exact URL and whether the
server (`Path::Api`) or the stream (`Path::Stream`) did the filtering. Nothing
is cached or written to disk; reads are capped at 64 MiB; HTTP 429, 5xx and
transport errors are retried with jitter. `latest_in_csv` parses any reader
for offline use. Country codes are uppercase ISO 3166-1 alpha-3; slugs are
`[a-z0-9-]`.

## Terms

Checked 2026-11-23 against OWID's
[reuse terms](https://ourworldindata.org/about#how-can-i-use-or-reuse-your-data-visualizations-articles-or-material)
and the grapher CSV endpoints themselves.

| Question | Finding |
| --- | --- |
| Automated access allowed? | The `.csv` endpoints are public, keyless and documented for download; no prohibition on automated reads. Charts built on non-redistributable data answer HTTP 403 and are not served. |
| Redistribution | CC BY 4.0 for OWID's processing; underlying data keeps the original producer's licence as named on each chart. |
| Attribution | Required: credit Our World in Data and the underlying source on the chart. |
| Rate limits | None published. Use the client's retries and avoid bulk crawling. |

The client bundles no upstream data. Fixtures are synthetic.
