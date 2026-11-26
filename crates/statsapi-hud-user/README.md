# statsapi-hud-user

A small blocking Rust client for HUD USER Fair Market Rent, Income Limits, and USPS crosswalk APIs.

## Install

```toml
[dependencies]
statsapi-hud-user = { git = "https://github.com/davidawad/statsapi", package = "statsapi-hud-user" }
```

## Use

The same example is in [`examples/basic.rs`](examples/basic.rs):

```rust,no_run
use statsapi_hud_user::HudUserClient;

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let token = std::env::var("HUD_API_TOKEN")?;
let client = HudUserClient::builder(token)
    .cache_dir(std::env::temp_dir().join("hud-user-cache"))
    .build()?;
let fmr = client.fmr("3402399999", None)?;
println!("{}", fmr.pointer("/data/basicdata/Two-Bedroom").unwrap_or(&serde_json::Value::Null));
let crosswalk = client.zip_crosswalk("08840", 1, Some(2025), Some(1))?;
println!("{}", crosswalk.pointer("/data/results").and_then(serde_json::Value::as_array).map_or(0, Vec::len));
# Ok(())
# }
```

Pass the HUD USER API token to `HudUserClient`; the library never reads credentials from process-global state. A cache is optional and its directory is supplied by the caller. It can be disabled by using `HudUserClient::new(token)`. Use the builder's `offline()` option to allow cache hits only; it never reads global environment state. Callers should avoid exceeding HUD's API rate limit.

## Auth, endpoints, and files

The client sends the token as a bearer credential and uses blocking `reqwest` with rustls. It covers `fmr/data/{entityid}` for Fair Market Rent and Small Area FMR, `il/data/{entityid}` for Income Limits, and `usps?type={n}&query={zip}` for crosswalk types 1 through 12. Responses are decoded as JSON and returned in HUD's response layout. Transient transport errors, HTTP 429, and HTTP 5xx responses receive bounded exponential retry with jitter. Optional caching is URL-keyed, local to the caller's supplied directory, and supports cache-only operation through the client builder's `offline()` option.

Offline fixtures in `tests/fixtures/` retain HUD's response layout and contain synthesized values, not copied upstream values. The ignored live test needs `HUD_API_TOKEN`; when absent it prints `credential_missing: HUD_API_TOKEN` and exits successfully.

## HUD USER API terms

Terms checked 2026-11-22 against the [FMR and Income Limits API documentation](https://www.huduser.gov/portal/dataset/fmr-api.html), [USPS Crosswalk API documentation](https://www.huduser.gov/portal/dataset/uspszip-api.html), and [HUD User API Terms of Service](https://www.huduser.gov/portal/dataset/api-terms-of-service.html). The service terms say the API may be used to develop a service to search, display, analyze, retrieve, and view HUD USER information; they do not forbid automated API access or the client’s purpose. HUD can change these terms, so review them before use.

| Question | HUD USER terms |
| --- | --- |
| Automated access allowed? | Yes, through the API with an account token; limit is 60 queries per minute. |
| Redistribution of data | No special restrictions on USPS crosswalk content are stated. FMR/Income Limits API terms do not state a separate redistribution grant; retain HUD's attribution and do not modify or falsely represent API content. |
| Attribution required? | Yes. Services using the API should prominently display: “This product uses the HUD User Data API but is not endorsed or certified by HUD User.” |
| Rate limits | Maximum 60 queries per minute; HUD may block access for attempts to exceed or circumvent limits. |

