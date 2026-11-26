# statsapi-epa-aqs

`statsapi-epa-aqs` is a blocking Rust client for the U.S. Environmental Protection Agency Air Quality System API.

## Install

```toml
[dependencies]
statsapi-epa-aqs = { git = "https://github.com/davidawad/statsapi", package = "statsapi-epa-aqs" }
```

## Usage

The same flow is compiled from [`examples/basic.rs`](examples/basic.rs). Set `EPA_AQS_EMAIL` and `EPA_AQS_KEY` before running it.

```rust,no_run
use std::env;
use statsapi_epa_aqs::{response_rows, Client, Credentials};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let credentials = Credentials::new(env::var("EPA_AQS_EMAIL")?, env::var("EPA_AQS_KEY")?)?;
    let client = Client::new(credentials)?;
    let query = client.query("pm25_annual_mean", "county", "06037", None, None, 2024)?;
    let rows = response_rows(&client.fetch(&query)?)?;
    println!("{} rows", rows.len());
    Ok(())
}
```

`Credentials` are supplied by the caller. The example reads the registration email and key from the environment and never contains a credential.

## Coverage

| Upstream endpoint or file | Supported access |
| --- | --- |
| AQS annual summary | PM2.5 annual mean and ozone fourth-highest eight-hour value by county or CBSA |
| AQS daily summary | Days over AQI 100 and nearest PM2.5 daily queries by county or coordinate box |
| AQS JSON responses | Success validation, row extraction, credential-free reporting URLs, and numeric cells |

## EPA AQS terms

Terms reviewed 2026-11-26. This summary does not replace the [AQS API documentation and User Limits and Terms of Service](https://aqs.epa.gov/aqsweb/documents/data_api.html).

| Question | Terms summary |
| --- | --- |
| Automated access allowed? | Yes. The API is provided for application developers and data analysts, and a registered email/key is required. |
| Redistribution of data | AQS data may be obtained by API users; confirm the current EPA conditions before redistributing a derived product. |
| Attribution required? | Identify the EPA Air Quality System as the source and preserve the public query URL when reporting results. |
| Rate limits | EPA asks clients to wait five seconds between requests and stay within ten requests per minute; callers own pacing and caching. |
