# Showcase

Every image here is generated offline: no network, no API keys, no upstream calls. Each crate's `examples/showcase.rs` decodes that crate's synthetic test fixture (the real upstream layout, invented values) through its public API and prints a small table; the script wraps the text in a terminal frame and renders the PNG.

```sh
docs/showcase/make-screenshots.sh           # both stages
docs/showcase/make-screenshots.sh --text    # run every example (needs cargo)
docs/showcase/make-screenshots.sh --render  # SVG + PNG (needs rsvg-convert: brew install librsvg)
```

Every client also has `examples/basic.rs`, which calls the live upstream (network, and a key where its README says so).

## `statsapi-arcgis`

Reusable blocking client for ArcGIS REST feature services

![statsapi-arcgis showcase output](showcase/statsapi-arcgis.png)

## `statsapi-bea`

Reusable blocking Rust client for the U.S. Bureau of Economic Analysis Regional API

![statsapi-bea showcase output](showcase/statsapi-bea.png)

## `statsapi-bis`

Reusable client for the keyless BIS Data Portal SDMX API: credit gaps, total credit, property prices, debt service ratios

![statsapi-bis showcase output](showcase/statsapi-bis.png)

## `statsapi-bls`

Reusable blocking Rust client for the U.S. Bureau of Labor Statistics public data APIs

![statsapi-bls showcase output](showcase/statsapi-bls.png)

## `statsapi-cdc-wonder`

Reusable blocking client for the CDC WONDER XML API and CDC mortality datasets on data.cdc.gov

![statsapi-cdc-wonder showcase output](showcase/statsapi-cdc-wonder.png)

## `statsapi-census`

Reusable client and parsers for Census API and Census Reporter ACS data

![statsapi-census showcase output](showcase/statsapi-census.png)

## `statsapi-census-bds`

Reusable blocking Rust client for the U.S. Census Bureau Business Dynamics Statistics (BDS) timeseries API

![statsapi-census-bds showcase output](showcase/statsapi-census-bds.png)

## `statsapi-cfpb`

Reusable client for CFPB complaint and consumer credit trends data.

![statsapi-cfpb showcase output](showcase/statsapi-cfpb.png)

## `statsapi-cisa-kev`

Reusable blocking Rust client for the CISA Known Exploited Vulnerabilities catalog and NVD CVE counts

![statsapi-cisa-kev showcase output](showcase/statsapi-cisa-kev.png)

## `statsapi-cms`

Reusable client for the CMS data.cms.gov data-api v1 (Medicare geographic variation, Part D opioid prescribing)

![statsapi-cms showcase output](showcase/statsapi-cms.png)

## `statsapi-county-health-rankings`

Reusable client for County Health Rankings & Roadmaps annual data

![statsapi-county-health-rankings showcase output](showcase/statsapi-county-health-rankings.png)

## `statsapi-cow`

Reusable CSV decoder for Correlates of War historical datasets

![statsapi-cow showcase output](showcase/statsapi-cow.png)

## `statsapi-dataverse`

Reusable Rust client for Dataverse dataset and file APIs

![statsapi-dataverse showcase output](showcase/statsapi-dataverse.png)

## `statsapi-drought-monitor`

Reusable blocking client for the US Drought Monitor data services

![statsapi-drought-monitor showcase output](showcase/statsapi-drought-monitor.png)

## `statsapi-ember`

Reusable blocking client that streams Ember electricity generation CSV downloads

![statsapi-ember showcase output](showcase/statsapi-ember.png)

## `statsapi-envirofacts`

Reusable blocking Rust client for EPA Envirofacts (TRI) and the EPA Superfund NPL ArcGIS layer

![statsapi-envirofacts showcase output](showcase/statsapi-envirofacts.png)

## `statsapi-epa-aqs`

Reusable blocking client for the EPA Air Quality System API

![statsapi-epa-aqs showcase output](showcase/statsapi-epa-aqs.png)

## `statsapi-eurostat`

Reusable blocking client for the Eurostat dissemination API (JSON-stat) and GISCO NUTS boundaries

![statsapi-eurostat showcase output](showcase/statsapi-eurostat.png)

## `statsapi-eviction-lab`

Reusable blocking client for Eviction Lab aggregate data

![statsapi-eviction-lab showcase output](showcase/statsapi-eviction-lab.png)

## `statsapi-faostat`

Reusable blocking client that streams FAOSTAT bulk datasets and filters one country in memory

![statsapi-faostat showcase output](showcase/statsapi-faostat.png)

## `statsapi-fars`

Reusable blocking client for NHTSA FARS annual CSV archives.

![statsapi-fars showcase output](showcase/statsapi-fars.png)

## `statsapi-fbi-cde`

Blocking Rust client for the FBI Crime Data Explorer summarized-offense API

![statsapi-fbi-cde showcase output](showcase/statsapi-fbi-cde.png)

## `statsapi-fcc-bdc`

Reusable blocking client for FCC Broadband Data Collection public files

![statsapi-fcc-bdc showcase output](showcase/statsapi-fcc-bdc.png)

## `statsapi-fema`

Reusable blocking client for OpenFEMA and FEMA public data services

![statsapi-fema showcase output](showcase/statsapi-fema.png)

## `statsapi-fhfa`

Reusable streaming client for FHFA annual house price index files by ZIP, county and tract

![statsapi-fhfa showcase output](showcase/statsapi-fhfa.png)

## `statsapi-gdacs`

Reusable blocking client for the GDACS global disaster alert API

![statsapi-gdacs showcase output](showcase/statsapi-gdacs.png)

## `statsapi-gdelt`

Reusable blocking client for GDELT DOC 2.0 timelines

![statsapi-gdelt showcase output](showcase/statsapi-gdelt.png)

## `statsapi-governance-indices`

Reusable blocking client for the Transparency International CPI and RSF World Press Freedom Index

![statsapi-governance-indices showcase output](showcase/statsapi-governance-indices.png)

## `statsapi-hmda`

Reusable Rust client for the FFIEC HMDA Data Browser CSV API.

![statsapi-hmda showcase output](showcase/statsapi-hmda.png)

## `statsapi-household-credit`

Reusable client for public household credit data from the NY Fed, Federal Reserve Board, and Urban Institute

![statsapi-household-credit showcase output](showcase/statsapi-household-credit.png)

## `statsapi-hud-user`

Reusable blocking Rust client for the HUD USER FMR, income limits, and USPS crosswalk APIs

![statsapi-hud-user showcase output](showcase/statsapi-hud-user.png)

## `statsapi-imf-datamapper`

Reusable blocking client for the IMF DataMapper API (World Economic Outlook series)

![statsapi-imf-datamapper showcase output](showcase/statsapi-imf-datamapper.png)

## `statsapi-ioda`

Reusable blocking client for the IODA internet outage API

![statsapi-ioda showcase output](showcase/statsapi-ioda.png)

## `statsapi-irs-eo`

Reusable parsers and blocking access helpers for IRS exempt-organization files

![statsapi-irs-eo showcase output](showcase/statsapi-irs-eo.png)

## `statsapi-irs-soi`

Reusable client for IRS Statistics of Income ZIP and migration datasets

![statsapi-irs-soi showcase output](showcase/statsapi-irs-soi.png)

## `statsapi-launch-library`

Reusable blocking client for The Space Devs Launch Library 2 launch counts and upcoming launches

![statsapi-launch-library showcase output](showcase/statsapi-launch-library.png)

## `statsapi-meta-sci`

Reusable client for Meta's Social Connectedness Index files on HDX

![statsapi-meta-sci showcase output](showcase/statsapi-meta-sci.png)

## `statsapi-naep`

Reusable Rust client for the NAEP Data Service API

![statsapi-naep showcase output](showcase/statsapi-naep.png)

## `statsapi-nara`

Reusable NARA Catalog API v2 client

![statsapi-nara showcase output](showcase/statsapi-nara.png)

## `statsapi-nasa-power`

Reusable blocking client for the NASA POWER daily point API

![statsapi-nasa-power showcase output](showcase/statsapi-nasa-power.png)

## `statsapi-noaa`

Reusable client for NOAA NCEI Storm Events bulk files

![statsapi-noaa showcase output](showcase/statsapi-noaa.png)

## `statsapi-noaa-cdo`

Reusable blocking client for the NOAA NCEI Access Data Service (GHCN-Daily summaries and 1991-2020 climate normals)

![statsapi-noaa-cdo showcase output](showcase/statsapi-noaa-cdo.png)

## `statsapi-ons`

Reusable blocking client for the UK Office for National Statistics API and postcodes.io

![statsapi-ons showcase output](showcase/statsapi-ons.png)

## `statsapi-ookla`

Reusable parser and client helpers for Ookla Open Data performance tiles

![statsapi-ookla showcase output](showcase/statsapi-ookla.png)

## `statsapi-openaq`

Reusable blocking Rust client for the OpenAQ v3 air quality API

![statsapi-openaq showcase output](showcase/statsapi-openaq.png)

## `statsapi-openfec`

Blocking Rust client for OpenFEC aggregate and committee endpoints

![statsapi-openfec showcase output](showcase/statsapi-openfec.png)

## `statsapi-opensky`

Reusable blocking client for the OpenSky Network live state-vector REST API

![statsapi-opensky showcase output](showcase/statsapi-opensky.png)

## `statsapi-opportunity-insights`

Blocking client for Opportunity Insights public CSV datasets

![statsapi-opportunity-insights showcase output](showcase/statsapi-opportunity-insights.png)

## `statsapi-owid`

Reusable blocking streaming client for Our World in Data grapher CSV endpoints

![statsapi-owid showcase output](showcase/statsapi-owid.png)

## `statsapi-power-price`

Reusable client for public U.S. electricity price data

![statsapi-power-price showcase output](showcase/statsapi-power-price.png)

## `statsapi-reliefweb`

Reusable blocking client for the ReliefWeb API v2 disasters and reports endpoints

![statsapi-reliefweb showcase output](showcase/statsapi-reliefweb.png)

## `statsapi-sanctions`

Reusable streaming client for the OFAC SDN list and the UN Security Council consolidated list

![statsapi-sanctions showcase output](showcase/statsapi-sanctions.png)

## `statsapi-sba`

Reusable streaming Rust client for the SBA open data 7(a), 504 and PPP loan files.

![statsapi-sba showcase output](showcase/statsapi-sba.png)

## `statsapi-sdmx`

Reusable blocking SDMX HTTP client and response decoder

![statsapi-sdmx showcase output](showcase/statsapi-sdmx.png)

## `statsapi-seda`

Blocking CSV client for Stanford Education Data Archive releases

![statsapi-seda showcase output](showcase/statsapi-seda.png)

## `statsapi-senate-lda`

Reusable blocking client for the US Senate Lobbying Disclosure Act (LDA) REST API

![statsapi-senate-lda showcase output](showcase/statsapi-senate-lda.png)

## `statsapi-socrata`

Reusable blocking client for Socrata Open Data API

![statsapi-socrata showcase output](showcase/statsapi-socrata.png)

## `statsapi-statcan`

Reusable blocking client for the Statistics Canada Web Data Service (WDS)

![statsapi-statcan showcase output](showcase/statsapi-statcan.png)

## `statsapi-ucdp-history`

Reusable CSV decoder for UCDP/PRIO historical conflict datasets

![statsapi-ucdp-history showcase output](showcase/statsapi-ucdp-history.png)

## `statsapi-un-wpp`

Reusable blocking client that streams UN World Population Prospects CSV downloads

![statsapi-un-wpp showcase output](showcase/statsapi-un-wpp.png)

## `statsapi-urban-education`

Reusable client for the Urban Institute Education Data Portal

![statsapi-urban-education showcase output](showcase/statsapi-urban-education.png)

## `statsapi-uscourts-bankruptcy`

Blocking Rust client for U.S. Courts F-5A bankruptcy statistics

![statsapi-uscourts-bankruptcy showcase output](showcase/statsapi-uscourts-bankruptcy.png)

## `statsapi-usda-food-access`

Reusable client for USDA ERS food access and FNS SNAP data files

![statsapi-usda-food-access showcase output](showcase/statsapi-usda-food-access.png)

## `statsapi-usgs-earthquakes`

Reusable blocking client for the USGS Earthquake Catalog FDSN event API

![statsapi-usgs-earthquakes showcase output](showcase/statsapi-usgs-earthquakes.png)

## `statsapi-vdem`

Reusable V-Dem Country-Year dataset client

![statsapi-vdem showcase output](showcase/statsapi-vdem.png)

## `statsapi-who-gho`

Reusable blocking client for the WHO Global Health Observatory OData API

![statsapi-who-gho showcase output](showcase/statsapi-who-gho.png)

## `statsapi-wikidata-events`

Wikidata event and Wikipedia Current Events client

![statsapi-wikidata-events showcase output](showcase/statsapi-wikidata-events.png)

## `statsapi-wikimedia-hiringlab`

Reusable blocking client for Wikimedia pageviews and Indeed Hiring Lab job postings

![statsapi-wikimedia-hiringlab showcase output](showcase/statsapi-wikimedia-hiringlab.png)

## `statsapi-worldbank`

Reusable blocking client for the World Bank Indicators API v2

![statsapi-worldbank showcase output](showcase/statsapi-worldbank.png)

## `statsapi-wvs-pew`

Parsers and weighting for manually downloaded World Values Survey and Pew data files

![statsapi-wvs-pew showcase output](showcase/statsapi-wvs-pew.png)
