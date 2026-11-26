# statsapi-openaq

Blocking Rust client for the [OpenAQ v3 API](https://docs.openaq.org/): PM2.5
locations near a point or in a country, latest readings, and daily means.
No repo dependencies; `publish = false`, GPL-3.0-or-later. The adapter
`openaq-data` depends on it.

## Terms

| Item | Value |
| --- | --- |
| API root | `https://api.openaq.org/v3` |
| Key | **Required** on every request (`X-API-Key`); free at <https://explore.openaq.org/register> |
| Keyless route | None usable. v1/v2 are retired (HTTP 410); the public S3 archive (`openaq-data-archive`) is keyless but has no station index, so it cannot answer "nearest station to a place" |
| Data licence | Per-provider; OpenAQ aggregates are CC BY 4.0 (<https://docs.openaq.org/about/about>) |
| Rate limits | 60 requests/min, 2000/hour per key; country latest reads request each location separately with a one-second gap |
| Radius | `coordinates` queries are capped at 25 000 m; the client clamps |

## Fixtures

`tests/fixtures/*.json` are synthetic examples of the documented API response
shape, not captured live responses.
