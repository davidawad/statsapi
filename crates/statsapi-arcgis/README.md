# statsapi-arcgis

`statsapi-arcgis` is a blocking client for public ArcGIS REST feature-service JSON.

## Install

```toml
[dependencies]
statsapi-arcgis = { git = "https://github.com/davidawad/statsapi", package = "statsapi-arcgis" }
```

## Usage

```rust
use statsapi_arcgis::Client;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let response = Client::new().get_json(
        "https://services2.arcgis.com/QUAsjBqieHEMNnZW/ArcGIS/rest/services/FEMA_NRI/FeatureServer/0?f=json",
    )?;
    println!("{}", response["name"]);
    Ok(())
}
```

Pass complete REST URLs so the caller controls layer, SQL filter, and output fields. Optional response caching is content-addressed and owned by the caller. HTTP 429, 5xx, and transport failures use bounded jittered retries.

## Terms

| Source | Automated access | Redistribution and attribution | Rate limits |
|---|---|---|---|
| [ArcGIS REST API](https://developers.arcgis.com/rest/) | Public services support REST queries. | Service-specific; FEMA/NIFC/USFS federal records are public domain, with source attribution. | Service-specific; no common numeric limit. |
| [MTBS](https://mtbs.gov/faqs) | National perimeter products and REST views are public. | Federal data; credit MTBS / USFS / USGS. | No numeric limit stated. |

Tests use synthesized response values and do not copy upstream records.
