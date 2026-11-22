# statsapi-uscourts-bankruptcy

A blocking Rust client fetches and decodes U.S. Courts F-5A bankruptcy statistics into county rows.

## Install

```toml
[dependencies]
statsapi-uscourts-bankruptcy = { git = "https://github.com/davidawad/statsapi", package = "statsapi-uscourts-bankruptcy" }
```

## Usage

```rust,no_run
use statsapi_uscourts_bankruptcy::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let latest = client.latest()?;
    println!("{}: {} counties", latest.date, latest.counties.len());
    Ok(())
}
```

The same example is available at [`examples/basic.rs`](examples/basic.rs). The source is public and requires no API key. Use `Client::with_cache_dir(Some(path))` to supply an optional caller-owned content cache, or `Client::offline_cache(path)` to restrict reads to that cache. Requests retry transport failures, HTTP 429, and server errors with bounded jittered backoff.

## Coverage

The client reads the public bankruptcy listing, follows dated F-5A detail pages, downloads the linked XLSX workbooks, and decodes county FIPS, county labels, total filings, Chapter 7, Chapter 13, business, and nonbusiness counts. It exposes the reporting vintage and source URL. It does not access PACER case records.

## Upstream terms

Reviewed 2026-11-22: the [F-5A download listing](https://www.uscourts.gov/data-table-numbers/f-5a), [U.S. Courts privacy and security policy](https://www.uscourts.gov/privacy-security-policy), and [U.S. Courts homepage](https://www.uscourts.gov/). The F-5A listing provides public downloadable statistical workbooks. The reviewed site policy describes monitoring and security but does not state a numeric request limit or a specific F-5A redistribution or attribution condition. Federal government works are generally outside U.S. copyright under [17 U.S.C. § 105](https://uscode.house.gov/view.xhtml?req=granuleid:USC-prelim-title17-section105&num=0&edition=prelim); verify third-party material separately if present.

| Question | Finding |
| --- | --- |
| Automated access allowed? | No prohibition found for the public F-5A listing and workbook downloads; access is subject to the site's monitoring and security policy. This client uses low-volume public downloads with retry backoff. |
| Redistribution of data | No F-5A-specific restriction found in the reviewed terms. U.S. federal government works are generally public domain; third-party material, if any, can have separate rights. |
| Attribution required? | No requirement found in the reviewed F-5A terms; identifying U.S. Courts as the source is recommended. |
| Rate limits | No numeric rate limit found. Use the client for occasional reporting-period downloads and avoid excessive requests. |

## Decisions

