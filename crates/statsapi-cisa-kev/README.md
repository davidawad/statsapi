# statsapi-cisa-kev

Blocking Rust client that turns the CISA Known Exploited Vulnerabilities catalog and NVD CVE counts into window counts, ransomware shares and top vendors.

## Install

```toml
statsapi-cisa-kev = { git = "https://github.com/davidawad/statsapi" }
```

## Usage

`examples/basic.rs` compiles with the crate:

```rust
let client = Client::new()?;
let now = Utc::now().naive_utc();
let summary = client.fetch_catalog()?.summarize(now.date())?;
let (critical, _url) = client.critical_cves_published(now, 7)?;
```

## Auth

None. NVD allows keyless use at 5 requests per rolling 30 seconds; this client makes one request per call and retries 429 and 5xx with jittered backoff.

## Endpoints covered

- `https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json` (about 2 MiB). cisa.gov's CDN answers some clients with HTTP 403, so the client then tries CISA's own mirror, `raw.githubusercontent.com/cisagov/kev-data/develop/known_exploited_vulnerabilities.json`. The body is streamed through the decoder and never written to disk.
- `https://services.nvd.nist.gov/rest/json/cves/2.0` with `cvssV3Severity=CRITICAL` and a publication window of at most 120 days; only `totalResults` is read.

## Terms (checked 2026-11-23)

| Service | Automated access | Redistribution | Attribution | Rate limits |
| --- | --- | --- | --- | --- |
| CISA KEV ([catalog](https://www.cisa.gov/known-exploited-vulnerabilities-catalog), [terms](https://www.cisa.gov/news-events/news/cisa-kev-catalog-terms-use)) | Machine-readable JSON/CSV feeds are published for this purpose | US government work, free to use and share | Credit CISA as the source | None published |
| NVD ([API](https://nvd.nist.gov/developers/start-here), [terms](https://nvd.nist.gov/developers/terms-of-use)) | Public API, no key required | Public domain NIST data | Say that NVD is the source and that the product is not endorsed or certified by NVD | 5 requests / 30 s without a key, 50 with one |

## Tests

`cargo test -p statsapi-cisa-kev` runs offline over a synthetic fixture and a loopback server. `cargo test -p statsapi-cisa-kev -- --ignored` runs the live check.
