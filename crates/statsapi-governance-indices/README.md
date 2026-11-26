# statsapi-governance-indices

Pure blocking client (no repo dependencies; GPL-3.0-or-later, `publish = false`)
for two annual governance indices. Each edition is streamed into memory,
parsed, filtered by the caller and dropped: nothing is written to disk and no
bulk archive is kept. The newest edition is found by probing the publisher's
per-year URL from the current year down two years.

| Index | Source | Format | Terms | Status |
|---|---|---|---|---|
| Transparency International CPI | `https://images.transparencycdn.org/images/CPI<year>_Results.xlsx` (sheet "CPI Timeseries", columns `CPI score <year>`, `Rank <year>`) | XLSX | Data licensed CC BY-ND 4.0 (credit Transparency International; no derivatives). `robots.txt` bars only `/cpresources/`, `/vendor/`, `/cache/`. | served |
| RSF World Press Freedom Index | `https://rsf.org/sites/default/files/import_classement/<year>.csv` | CSV, `;` delimited, decimal commas | Credit RSF. `robots.txt` allows `/` for generic agents, blocks named AI crawlers and sets `ai-train=no, use=reference`; this client sends its own user agent and only looks values up for reference. | served |
| Vision of Humanity Global Peace Index | `visionofhumanity.org` downloads | XLSX behind a form | The site answers automated requests with HTTP 403 and publishes no stable direct file URL. | **skipped**: automated access is not available |

API: `GovernanceClient::latest(Index, current_year) -> Edition`,
`Edition::country("USA") -> Observation { score, rank, prior_score, prior_rank }`,
`Observation::score_change()` and `rank_change()` (prior rank minus rank, so
positive means moved up). `parse_cpi_xlsx` and `parse_rsf_csv` are pure and
tested over trimmed fixtures in `tests/fixtures/`. Live test:
`cargo test -p statsapi-governance-indices -- --ignored`.
