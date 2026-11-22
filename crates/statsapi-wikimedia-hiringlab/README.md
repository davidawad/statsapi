# statsapi-wikimedia-hiringlab

`statsapi-wikimedia-hiringlab` is a reusable blocking Rust client for Wikimedia pageviews and Indeed Hiring Lab job postings.

## Install

```toml
[dependencies]
statsapi-wikimedia-hiringlab = { git = "https://github.com/davidawad/statsapi", package = "statsapi-wikimedia-hiringlab" }
```

## Usage

See [examples/basic.rs](examples/basic.rs) for a compiling example:

```rust,no_run
use statsapi_wikimedia_hiringlab::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder("my-indicator/1.0 (contact: ops@example.org)")
        .cache_dir("./cache")
        .build()?;
    let title = client.wikipedia_title("Example Town")?;
    let views = client.pageviews(&title, "20260101", "20260131")?;
    println!("{} views from {}", views.total, views.source_url);
    let hiring = client.hiring_lab_observation(true, "US")?;
    println!("Hiring Lab index {} on {}", hiring.latest, hiring.latest_date);
    Ok(())
}
```

## Authentication

The covered endpoints are public and keyless. Supply a descriptive User-Agent with contact information as required by Wikimedia's API policies. There is no API key or credential environment variable.

## Endpoints and files

- Wikidata entity search (`wbsearchentities`) and entity JSON, to resolve a label to its English Wikipedia article.
- Wikimedia Pageviews REST API, per-article daily pageview totals.
- Indeed Hiring Lab US country aggregate CSV (`aggregate_job_postings_US.csv`) and US metro CSV (`metro_job_postings_us.csv`). Country queries use the `US` geography and the seasonally adjusted index; metro queries use a CBSA code.

The client uses blocking reqwest with rustls, retries transport failures and HTTP 429/5xx responses with jittered backoff, and optionally caches successful response bytes under a caller-supplied directory. The cache is keyed by SHA-256 of the request URL. No repository crate or global state is required.

## Terms

Terms reviewed 2026-11-22.

| Question | Wikimedia APIs | Indeed Hiring Lab data |
| --- | --- | --- |
| Automated access allowed? | Yes, subject to API usage guidelines, User-Agent identification, and rate limits. | Yes; the public repository provides its generated data for public use. |
| Redistribution of data? | Pageview metrics are reusable under the applicable free data license; the API access policy says licenses vary by API. | Yes, under CC BY 4.0. |
| Attribution required? | Pageview metrics are CC0; attribution is appreciated. Follow the license for any other Wikimedia content used. | Yes. Cite Indeed Hiring Lab as the source and meet CC BY 4.0 attribution terms. |
| Rate limits | Respect throttling and `Retry-After`, identify the client, and keep traffic reasonable; numerical limits can change. | The repository does not specify a numeric rate limit; fetch the published files responsibly. |

Sources: [Wikimedia REST API access policy and API etiquette](https://wikimedia.org/api/rest_v1/) (including its linked User-Agent policy); [Wikimedia Foundation API Usage Guidelines](https://foundation.wikimedia.org/wiki/Policy:API_usage_guidelines/en); [Indeed Hiring Lab job postings tracker and license](https://github.com/hiring-lab/job_postings_tracker).
