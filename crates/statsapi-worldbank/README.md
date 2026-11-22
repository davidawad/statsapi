# statsapi-worldbank

`statsapi-worldbank` is a reusable blocking Rust client for World Bank country metadata and indicator observations.

## Install

```toml
[dependencies]
statsapi-worldbank = { git = "https://github.com/davidawad/statsapi", package = "statsapi-worldbank" }
```

## Usage

This example is also available as [`examples/basic.rs`](examples/basic.rs):

```rust
use statsapi_worldbank::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(None)?;
    let response = client.indicator("USA", "SP.POP.TOTL", None)?;
    if let Some(observation) = Client::latest_observation(&response) {
        println!("{}: {}", observation["date"], observation["value"]);
    }
    Ok(())
}
```

## Authentication

The World Bank Indicators API v2 endpoints used here are public and require no key. `Client::new` accepts an optional cache directory; set `with_offline(true)` to require cached responses. No global environment variables or credentials are read by this client.

## Endpoints and data

- `GET /v2/country/{country}?format=json` decodes the country's World Bank region id and label.
- `GET /v2/country/{country-or-region}/indicator/{indicator}?format=json&mrnev=1[&source={database}]` returns the API's indicator response layout; `latest_observation` selects the newest non-null row.
- Response bodies are cached by URL digest when the caller supplies a directory. Requests retry transport errors, HTTP 429, and 5xx with bounded exponential delay and jitter.

## Terms

Terms reviewed 2026-11-22 at [World Bank Terms of Use for Datasets](https://www.worldbank.org/en/about/legal/terms-of-use-for-datasets) and the linked [World Bank Group terms and conditions](https://www.worldbank.org/ext/en/legal/terms-conditions).

| Question | Terms summary |
| --- | --- |
| Automated access allowed? | The World Bank describes APIs as tools for retrieving and using materials and allows use subject to specific dataset terms. Requests must stay within reasonable volume; excessive or abusive usage is prohibited. This client retries sparingly and users must respect those limits. |
| Redistribution of data | Depends on the dataset's specific terms and license. API terms prohibit commercial uses such as reselling, charging for access or redistribution, or charging for derivatives without permission. This crate distributes no upstream data. Check each dataset's license before redistributing returned values. |
| Attribution required? | Yes. The terms condition permitted material use on attribution to the World Bank Group, relevant member institutions, authors, and third-party providers. Retain dataset-specific attribution too. |
| Rate limits | No numeric rate is stated in the terms reviewed. Reasonable request volume is required; excessive or abusive use may be restricted. |

The terms permit reasonable API use and do not prohibit an independently implemented client for these endpoints. Use the data only under its dataset-specific terms and applicable noncommercial restrictions.
