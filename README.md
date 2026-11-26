# statsapi

Typed, blocking Rust clients for 26 public statistics sources (BLS, BEA, Census-adjacent federal datasets, World Bank, WHO, SDMX agencies, CDC and other Socrata portals, education, housing, credit, elections and more), so you can pull official numbers without writing a scraper per agency.

![Terminal output of the statsapi-bls showcase example: BLS LAUS and CES series, decoded from the crate's synthetic test fixture](docs/showcase/bls-terminal.png)

The image is the output of `cargo run -p statsapi-bls --example showcase`: the example decodes the crate's own synthetic test fixture (the real upstream layout, invented values) through the public API, offline. Reproduce it with `docs/showcase/make-screenshots.sh` (see [docs/showcase.md](docs/showcase.md)).

## Install

These crates are not on crates.io. Add the one you need as a git dependency:

```toml
[dependencies]
statsapi-bea = { git = "https://github.com/davidawad/statsapi", package = "statsapi-bea" }
```

Pin `rev = "<commit>"` or `tag = "<tag>"` for reproducible builds. Each crate is independent: depend only on the one you need. No crate reads global state; API keys and an optional cache directory are passed by the caller. HTTP is blocking `reqwest` with rustls (no OpenSSL), with jittered retry on 429, 5xx and transport errors.

Build everything: `cargo build --workspace`. The offline tests need no network or keys: `cargo test --workspace`. Live tests are `#[ignore]`d; run them with `cargo test -p <crate> -- --ignored` (keyed sources print `credential_missing` and pass when the key is absent).

## Crates

| Crate | What it covers | Details |
| --- | --- | --- |
| `ephemeral-cache` | TTL and size-capped last-resort cache for the statsapi clients (live API first) | [README](crates/ephemeral-cache/README.md) |
| `statsapi-arcgis` | Reusable blocking client for ArcGIS REST feature services | [README](crates/statsapi-arcgis/README.md) |
| `statsapi-bea` | Reusable blocking Rust client for the U.S. Bureau of Economic Analysis Regional API | [README](crates/statsapi-bea/README.md) |
| `statsapi-bis` | Reusable client for the keyless BIS Data Portal SDMX API: credit gaps, total credit, property prices, debt service ratios | [README](crates/statsapi-bis/README.md) |
| `statsapi-bls` | Reusable blocking Rust client for the U.S. Bureau of Labor Statistics public data APIs | [README](crates/statsapi-bls/README.md) |
| `statsapi-cdc-wonder` | Reusable blocking client for the CDC WONDER XML API and CDC mortality datasets on data.cdc.gov | [README](crates/statsapi-cdc-wonder/README.md) |
| `statsapi-census-bds` | Reusable blocking Rust client for the U.S. Census Bureau Business Dynamics Statistics (BDS) timeseries API | [README](crates/statsapi-census-bds/README.md) |
| `statsapi-cfpb` | Reusable client for CFPB complaint and consumer credit trends data. | [README](crates/statsapi-cfpb/README.md) |
| `statsapi-cisa-kev` | Reusable blocking Rust client for the CISA Known Exploited Vulnerabilities catalog and NVD CVE counts | [README](crates/statsapi-cisa-kev/README.md) |
| `statsapi-cms` | Reusable client for the CMS data.cms.gov data-api v1 (Medicare geographic variation, Part D opioid prescribing) | [README](crates/statsapi-cms/README.md) |
| `statsapi-county-health-rankings` | Reusable client for County Health Rankings & Roadmaps annual data | [README](crates/statsapi-county-health-rankings/README.md) |
| `statsapi-cow` | Reusable CSV decoder for Correlates of War historical datasets | [README](crates/statsapi-cow/README.md) |
| `statsapi-dataverse` | Reusable Rust client for Dataverse dataset and file APIs | [README](crates/statsapi-dataverse/README.md) |
| `statsapi-drought-monitor` | Reusable blocking client for the US Drought Monitor data services | [README](crates/statsapi-drought-monitor/README.md) |
| `statsapi-ember` | Reusable blocking client that streams Ember electricity generation CSV downloads | [README](crates/statsapi-ember/README.md) |
| `statsapi-envirofacts` | Reusable blocking Rust client for EPA Envirofacts (TRI) and the EPA Superfund NPL ArcGIS layer | [README](crates/statsapi-envirofacts/README.md) |
| `statsapi-eurostat` | Reusable blocking client for the Eurostat dissemination API (JSON-stat) and GISCO NUTS boundaries | [README](crates/statsapi-eurostat/README.md) |
| `statsapi-eviction-lab` | Reusable blocking client for Eviction Lab aggregate data | [README](crates/statsapi-eviction-lab/README.md) |
| `statsapi-faostat` | Reusable blocking client that streams FAOSTAT bulk datasets and filters one country in memory | [README](crates/statsapi-faostat/README.md) |
| `statsapi-fars` | Reusable blocking client for NHTSA FARS annual CSV archives. | [README](crates/statsapi-fars/README.md) |
| `statsapi-fbi-cde` | Blocking Rust client for the FBI Crime Data Explorer summarized-offense API | [README](crates/statsapi-fbi-cde/README.md) |
| `statsapi-fema` | Reusable blocking client for OpenFEMA and FEMA public data services | [README](crates/statsapi-fema/README.md) |
| `statsapi-fhfa` | Reusable streaming client for FHFA annual house price index files by ZIP, county and tract | [README](crates/statsapi-fhfa/README.md) |
| `statsapi-gdacs` | Reusable blocking client for the GDACS global disaster alert API | [README](crates/statsapi-gdacs/README.md) |
| `statsapi-gdelt` | Reusable blocking client for GDELT DOC 2.0 timelines | [README](crates/statsapi-gdelt/README.md) |
| `statsapi-gleif` | Reusable blocking client for the GLEIF LEI records API v1 | [README](crates/statsapi-gleif/README.md) |
| `statsapi-governance-indices` | Reusable blocking client for the Transparency International CPI and RSF World Press Freedom Index | [README](crates/statsapi-governance-indices/README.md) |
| `statsapi-hmda` | Reusable Rust client for the FFIEC HMDA Data Browser CSV API. | [README](crates/statsapi-hmda/README.md) |
| `statsapi-household-credit` | Reusable client for public household credit data from the NY Fed, Federal Reserve Board, and Urban Institute | [README](crates/statsapi-household-credit/README.md) |
| `statsapi-hud-user` | Reusable blocking Rust client for the HUD USER FMR, income limits, and USPS crosswalk APIs | [README](crates/statsapi-hud-user/README.md) |
| `statsapi-imf-datamapper` | Reusable blocking client for the IMF DataMapper API (World Economic Outlook series) | [README](crates/statsapi-imf-datamapper/README.md) |
| `statsapi-ioda` | Reusable blocking client for the IODA internet outage API | [README](crates/statsapi-ioda/README.md) |
| `statsapi-irs-soi` | Reusable client for IRS Statistics of Income ZIP and migration datasets | [README](crates/statsapi-irs-soi/README.md) |
| `statsapi-launch-library` | Reusable blocking client for The Space Devs Launch Library 2 launch counts and upcoming launches | [README](crates/statsapi-launch-library/README.md) |
| `statsapi-meta-sci` | Reusable client for Meta's Social Connectedness Index files on HDX | [README](crates/statsapi-meta-sci/README.md) |
| `statsapi-naep` | Reusable Rust client for the NAEP Data Service API | [README](crates/statsapi-naep/README.md) |
| `statsapi-nasa-power` | Reusable blocking client for the NASA POWER daily point API | [README](crates/statsapi-nasa-power/README.md) |
| `statsapi-noaa-cdo` | Reusable blocking client for the NOAA NCEI Access Data Service (GHCN-Daily summaries and 1991-2020 climate normals) | [README](crates/statsapi-noaa-cdo/README.md) |
| `statsapi-noaa` | Reusable client for NOAA NCEI Storm Events bulk files | [README](crates/statsapi-noaa/README.md) |
| `statsapi-nws` | Reusable blocking client for the NOAA National Weather Service alerts and forecast API | [README](crates/statsapi-nws/README.md) |
| `statsapi-ons` | Reusable blocking client for the UK Office for National Statistics API and postcodes.io | [README](crates/statsapi-ons/README.md) |
| `statsapi-openaq` | Reusable blocking Rust client for the OpenAQ v3 air quality API | [README](crates/statsapi-openaq/README.md) |
| `statsapi-openfec` | Blocking Rust client for OpenFEC aggregate and committee endpoints | [README](crates/statsapi-openfec/README.md) |
| `statsapi-opensky` | Reusable blocking client for the OpenSky Network live state-vector REST API | [README](crates/statsapi-opensky/README.md) |
| `statsapi-opportunity-insights` | Blocking client for Opportunity Insights public CSV datasets | [README](crates/statsapi-opportunity-insights/README.md) |
| `statsapi-owid` | Reusable blocking streaming client for Our World in Data grapher CSV endpoints | [README](crates/statsapi-owid/README.md) |
| `statsapi-power-price` | Reusable client for public U.S. electricity price data | [README](crates/statsapi-power-price/README.md) |
| `statsapi-reliefweb` | Reusable blocking client for the ReliefWeb API v2 disasters and reports endpoints | [README](crates/statsapi-reliefweb/README.md) |
| `statsapi-sanctions` | Reusable streaming client for the OFAC SDN list and the UN Security Council consolidated list | [README](crates/statsapi-sanctions/README.md) |
| `statsapi-sba` | Reusable streaming Rust client for the SBA open data 7(a), 504 and PPP loan files. | [README](crates/statsapi-sba/README.md) |
| `statsapi-sdmx` | Reusable blocking SDMX HTTP client and response decoder | [README](crates/statsapi-sdmx/README.md) |
| `statsapi-seda` | Blocking CSV client for Stanford Education Data Archive releases | [README](crates/statsapi-seda/README.md) |
| `statsapi-senate-lda` | Reusable blocking client for the US Senate Lobbying Disclosure Act (LDA) REST API | [README](crates/statsapi-senate-lda/README.md) |
| `statsapi-socrata` | Reusable blocking client for Socrata Open Data API | [README](crates/statsapi-socrata/README.md) |
| `statsapi-statcan` | Reusable blocking client for the Statistics Canada Web Data Service (WDS) | [README](crates/statsapi-statcan/README.md) |
| `statsapi-ucdp-history` | Reusable CSV decoder for UCDP/PRIO historical conflict datasets | [README](crates/statsapi-ucdp-history/README.md) |
| `statsapi-un-wpp` | Reusable blocking client that streams UN World Population Prospects CSV downloads | [README](crates/statsapi-un-wpp/README.md) |
| `statsapi-urban-education` | Reusable client for the Urban Institute Education Data Portal | [README](crates/statsapi-urban-education/README.md) |
| `statsapi-uscourts-bankruptcy` | Blocking Rust client for U.S. Courts F-5A bankruptcy statistics | [README](crates/statsapi-uscourts-bankruptcy/README.md) |
| `statsapi-usda-food-access` | Reusable client for USDA ERS food access and FNS SNAP data files | [README](crates/statsapi-usda-food-access/README.md) |
| `statsapi-usgs-earthquakes` | Reusable blocking client for the USGS Earthquake Catalog FDSN event API | [README](crates/statsapi-usgs-earthquakes/README.md) |
| `statsapi-who-gho` | Reusable blocking client for the WHO Global Health Observatory OData API | [README](crates/statsapi-who-gho/README.md) |
| `statsapi-wikimedia-hiringlab` | Reusable blocking client for Wikimedia pageviews and Indeed Hiring Lab job postings | [README](crates/statsapi-wikimedia-hiringlab/README.md) |
| `statsapi-worldbank` | Reusable blocking client for the World Bank Indicators API v2 | [README](crates/statsapi-worldbank/README.md) |
| `statsapi-wvs-pew` | Parsers and weighting for manually downloaded World Values Survey and Pew data files | [README](crates/statsapi-wvs-pew/README.md) |

## License

GPL-3.0-or-later; see [LICENSE](LICENSE).
