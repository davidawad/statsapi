# statsapi-sba

`statsapi-sba` streams the SBA open data 7(a), 504 and PPP loan-level CSVs row by row, so a caller can filter by ZIP or county without ever storing the multi-hundred-megabyte files.

## Install

```toml
[dependencies]
statsapi-sba = { git = "https://github.com/davidawad/statsapi", package = "statsapi-sba" }
```

## Usage

```rust
use statsapi_sba::Client;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(Duration::from_secs(300))?;
    let resources = client.discover()?; // current file URLs from the dataset pages
    let mut count = 0_u32;
    client.stream_loans(&resources.five_oh_four, |loan| {
        if loan.zip == "08840" {
            count += 1;
        }
    })?;
    println!("504 approvals with borrower ZIP 08840: {count}");
    Ok(())
}
```

(`examples/stream_504.rs`.) `decode_loans` and `decode_ppp` decode any `Read`, so callers can supply their own transport.

## Auth

None. SBA open data is keyless. Requests carry a `statsapi-sba/<version>` User-Agent (the portal answers 403 to requests with none).

## Endpoints covered

| Service | Files | Rows |
|---|---|---|
| [7(a) and 504 FOIA](https://data.sba.gov/dataset/7a-504-foia) | `FOIA_7a_FY2020_Present_asof_<YYMMDD>.csv`, `FOIA_504_FY2010_Present_asof_<YYMMDD>.csv` (about 190 MB and 60 MB), refreshed monthly | `Loan`: program, borrower ZIP5, project county and state, gross approval, approval date, NAICS code and title |
| [PPP FOIA](https://data.sba.gov/dataset/ppp-foia) | `public_150k_plus_<YYMMDD>.csv` and `public_up_to_150k_<n>_<YYMMDD>.csv` shards (about 5 GB together; static since 2024) | `PppLoan`: project ZIP5 (borrower ZIP when blank), project county and state, current approval amount |

Not covered: the data.sba.gov CKAN API (`/api/3/action/*` answers 404 since the portal moved to Drupal), the 7(a)/504 activity reports, EIDL and other datasets. PPP CSVs are not always UTF-8; fields decode lossily.

The client keeps no cache and writes nothing to disk. It retries 429, 5xx and transport errors with jitter when opening a request; a stream that breaks mid-body returns an error rather than partial rows.

## Terms

| Source | Automated access | Redistribution and attribution | Rate limits |
|---|---|---|---|
| [SBA open data](https://data.sba.gov/) | Public files, no key, no robots restriction on the dataset downloads. | Dataset license "U.S. Government Works" (public domain); credit the U.S. Small Business Administration. FOIA files withhold some borrower names ("Exemption 6"). | None stated; files are served from CloudFront. Stream once per request and filter; do not re-download in loops. |

Fixtures under `tests/fixtures/` are synthesized (`ZZ` state, invented borrowers); no SBA rows are copied.
