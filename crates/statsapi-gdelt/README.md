# statsapi-gdelt

`statsapi-gdelt` is a reusable blocking Rust client for GDELT DOC timeline queries, decoding, retries, and optional caller-owned caching.

## Install

```toml
[dependencies]
statsapi-gdelt = { git = "https://github.com/davidawad/statsapi", package = "statsapi-gdelt" }
```

## Usage

```rust,no_run
use statsapi_gdelt::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::with_cache_dir(Some(std::path::Path::new("./gdelt-cache")))?;
    let points = client.timeline("\"Paris\" sourcecountry:FR", "timelinevolraw", "20261101", "20261122")?;
    for point in points {
        println!("{}: {}", point.date, point.value);
    }
    Ok(())
}
```

The same request is in [`examples/basic.rs`](examples/basic.rs). GDELT DOC is public and does not require an API key; the client takes no credentials. Use `Client::new()` for no cache, or pass a directory owned by the caller to `with_cache_dir`. Cached response bodies are keyed by SHA-256 of the complete request URL and stored without a lock; entries remain reusable for at least 15 minutes. Call `offline_cache_only()` to use only cached responses. The client uses blocking `reqwest` with rustls, paces every request in the process at least six seconds apart, and retries HTTP 429, 5xx, transport failures, and GDELT's plain-text "Please limit requests to one every 5 seconds" response. Retry delays use bounded exponential backoff with jitter, starting between 5 and 8 seconds.

## Coverage

The client builds requests for GDELT DOC 2.0's `/api/v2/doc/doc` endpoint and supports its `timelinevolraw` and `timelinetone` modes. These return daily raw article-volume and tone timelines. Fixtures in `tests/fixtures/` preserve DOC's response layout and use synthetic dates and values only.

## Terms

Terms checked 2026-11-22: [GDELT Terms of Use](https://www.gdeltproject.org/about.html#termsofuse); the API's pacing guidance is also documented in the existing `gdelt-data` source notes.

| Question | Answer |
|---|---|
| Automated access allowed? | Yes. GDELT describes its platform as open for unrestricted academic, commercial, and governmental use. |
| Redistribution of data | Yes. Datasets may be redistributed, rehosted, republished, and mirrored in any form. |
| Attribution required? | Yes. Cite the GDELT Project and link to https://www.gdeltproject.org/. |
| Rate limits | Terms publish no numeric rate limit. DOC API guidance asks clients to leave at least five seconds between requests; this client enforces a six-second per-process floor. |

