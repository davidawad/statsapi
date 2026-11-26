# statsapi-dataverse

`statsapi-dataverse` gives Rust applications reusable, blocking access to Dataverse datasets, files, guestbooks, and signed downloads.

## Install

```toml
[dependencies]
statsapi-dataverse = { git = "https://github.com/davidawad/statsapi", package = "statsapi-dataverse" }
```

## Usage

This example is mirrored by [`examples/basic.rs`](examples/basic.rs) and is compiled by the crate build.

```rust
use statsapi_dataverse::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new("https://dataverse.harvard.edu")?;
    let dataset = client.get_json(
        "datasets/:persistentId/",
        &[("persistentId", "doi:10.7910/DVN/EXAMPLE")],
    )?;
    println!("{}", serde_json::to_string_pretty(&dataset)?);
    Ok(())
}
```

## Authentication and covered endpoints

Public metadata and files can be read without a key. Set `DATAVERSE_API_KEY` or pass `.with_api_key(key)` for endpoints or restricted files that need a Dataverse API key; the client sends it as `X-Dataverse-key` only to the configured Dataverse host. API keys are never printed. The base URL is caller-selected, so the same client can target other Dataverse installations.

`get_json` handles JSON routes such as dataset lookup (`/api/datasets/:persistentId/`), dataset and file metadata, and guestbook definitions. `download_file` reads `/api/access/datafile/{id}` directly or submits a caller-provided `GuestbookResponse` and downloads the returned short-lived signed URL. Guestbook name, email, institution, position, and question answers must be accurate and satisfy the installation's questions. The client does not invent or store identity details. The client preserves upstream JSON layouts and returns file bytes without imposing a dataset-specific schema.

Retries use bounded jitter for transport failures, HTTP 429, and 5xx responses. A cache is disabled by default; `.with_cache_dir(path)` opts into caller-managed content-addressed caching and `.offline()` requires cache hits. Cached files may contain upstream data, so callers remain responsible for dataset-specific terms and cache retention.

Applications using this third-party API client must make the [Dataverse API Terms of Use](https://support.dataverse.harvard.edu/harvard-dataverse-api-terms-use), [General Terms of Use](https://support.dataverse.harvard.edu/harvard-dataverse-general-terms-use), and [Community Norms](https://dataverse.org/best-practices/dataverse-community-norms/) accessible to their users and clearly state that the application is not affiliated with or endorsed by Harvard Dataverse.

## Terms

Reviewed 2026-11-22: [Dataverse API documentation](https://guides.dataverse.org/en/latest/api/dataaccess.html), [Harvard Dataverse API Terms](https://support.dataverse.harvard.edu/harvard-dataverse-api-terms-use), [Harvard Dataverse General Terms](https://support.dataverse.harvard.edu/harvard-dataverse-general-terms-use), and [CC0 1.0 dataset terms](https://creativecommons.org/publicdomain/zero/1.0/deed.en).

| Question | Finding |
| --- | --- |
| Automated access allowed? | Yes. The Data Access API explicitly provides programmatic downloads and documents guestbook submission and signed URLs. The API terms prohibit unreasonable bandwidth and service impairment, and require accurate identity and legal use. |
| Redistribution of data | Dataset-level terms control. CC0 datasets may be copied and redistributed; Harvard API terms prohibit selling, leasing, or sublicensing downloaded datasets or information. The client distributes no datasets, and callers must check each dataset's license and restrictions. |
| Attribution required? | CC0 does not require attribution. Other dataset licenses may; attribution/citation is recommended and each dataset's terms apply. |
| Rate limits | No numeric rate limit is stated in the reviewed API or general terms. API terms require reasonable bandwidth and prohibit impairing service; the client retries 429/5xx with bounded delays. |

This client is generic Dataverse API software, not an MIT Election Lab client. It does not grant rights to any dataset, bypass access restrictions, or authorize selling, leasing, sublicensing, or monetizing API access. Free reuse of the client code does not imply Harvard affiliation.

