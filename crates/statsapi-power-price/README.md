# statsapi-power-price

statsapi-power-price fetches and decodes OpenEI ZIP utility-rate releases and EIA state monthly electricity prices.

Install from the reusable client repository: statsapi-power-price = { git = "https://github.com/davidawad/statsapi", package = "statsapi-power-price" }.

Usage (examples/basic.rs): Client::new().with_cache_dir(path).zip_rates(*VINTAGES.last()?) returns typed rows with utility identity, service type, and sector rates. Client::eia_monthly(state, sector, api_key) returns state-sector price observations. Keys are passed by the caller and are not retained.

## Endpoints

- OpenEI/NLR annual CSV releases: https://data.openei.org/files/{submission}/{iou-or-non-iou-csv}.
- EIA-861 annual utility sales/revenue archives: https://www.eia.gov/electricity/data/eia861/archive/zip/f861{year}.zip (2010 uses the legacy f86110.zip archive and file2_2010.xls workbook).
- EIA-861M monthly state workbook: https://www.eia.gov/electricity/data/eia861m/xls/sales_revenue.xlsx.
- EIA API v2: /v2/electricity/retail-sales/data/ filtered by state and sector, as a keyed fallback.

The client uses blocking reqwest with rustls, retries transport failures, HTTP 429 and 5xx with jitter, and offers optional caller-owned URL-keyed SHA-256 caches. Set POWER_PRICE_DATA_OFFLINE=1 to require cache hits.

## Terms

| Source | Automated access | Redistribution | Attribution | Rate limits |
| --- | --- | --- | --- | --- |
| OpenEI/NLR ZIP utility rates | Public downloadable CSV | Dataset marked publicly accessible; retain the source citation | Cite NLR/OpenEI and the release | No explicit limit published |
| EIA electricity API | Public API; key can be supplied | Public U.S. government data | Cite EIA and data series | Follow EIA API terms and published limits |

Sources: [OpenEI 2025 release](https://data.openei.org/submissions/10259), [EIA API terms](https://www.eia.gov/opendata/).

## Known releases

OpenEI search and each submission's resource list confirmed releases for 2011, 2013–2025; no 2010 or 2012 submission appeared in the portal search. The client includes all confirmed downloadable annual releases. EIA-861 annual utility files provide revenue/sales rates for a missing vintage and a latest-year cross-check. URDB tariff-level details are out of scope because they require an NREL API key.

## Offline tests

Offline tests use a synthesized CSV preserving the OpenEI header layout. The ignored live test downloads a current release and state observations when run in the pod.
