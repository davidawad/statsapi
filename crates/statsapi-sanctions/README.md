# statsapi-sanctions

`statsapi-sanctions` is a reusable blocking Rust client that streams the OFAC SDN list and the UN Security Council consolidated list and reports how many listed entities and individuals are tied to a country, how many were added in the last 90 days, and which programs touch it.

## Install

```toml
[dependencies]
statsapi-sanctions = { git = "https://github.com/davidawad/statsapi", package = "statsapi-sanctions" }
```

## Usage

The same flow is in [`examples/basic.rs`](examples/basic.rs):

```rust
use statsapi_sanctions::{CountryNames, Date, SanctionsClient};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let names = CountryNames::for_alpha3("IRN").ok_or("unknown country")?;
    let today = Date::parse_iso("2026-11-23")?;
    let client = SanctionsClient::new()?;
    let un = client.un_exposure(&names, today)?;
    println!("UN: {} entities, {} individuals", un.entities, un.individuals);
    Ok(())
}
```

Both lists are keyless bulk XML files with no query API, so the client streams
each file, filters it to the country while parsing, and keeps nothing: no
cache, no archive. `scan_ofac_sdn`, `recent_ofac_profiles` and `scan_un` take
any `BufRead`, so you can run them over a file you already hold.

## Coverage

| Call | Reads | Gives |
| --- | --- | --- |
| `ofac_exposure` | `SDN.XML` (about 30 MB) and `SDN_ADVANCED.XML` (about 130 MB) | entities and individuals with an address or nationality in the country, programs, additions in 90 days |
| `un_exposure` | `consolidated.xml` (about 2 MB) | the same, with the UN list type as the program |

The legacy SDN export carries no dates; listing dates come from the advanced
export's `EntryEvent`. If that second stream fails, `additions` is `None` and
the counts still return. "Tied to the country" means any address country or
nationality equals it (UN individuals: `NATIONALITY` and `INDIVIDUAL_ADDRESS`;
UN entities: `ENTITY_ADDRESS`). Place of birth and citizenship are not used.
Country values are matched by normalized name against the ISO short name and
the spellings the lists use (`Burma`, `Korea, North`, `Region: Iran`);
`CountryNames::for_alpha3` returns `None` for a code it does not know.
Vessels and aircraft are not counted as entities. Retries use jitter for HTTP
429, 5xx and transport errors.

## Terms

Checked 2026-11-23 against the [OFAC Sanctions List Service](https://ofac.treasury.gov/sanctions-list-service)
and the [UN Security Council sanctions lists](https://main.un.org/securitycouncil/en/content/un-sc-consolidated-list).

| Question | Finding |
| --- | --- |
| Automated access allowed? | OFAC publishes the files for download by software and says so on the service page; the UN publishes `consolidated.xml` at a stable URL for the same purpose. Neither requires a key or login. |
| Redistribution | OFAC data is a U.S. government work (public domain). UN list content is reusable with attribution to the UN Security Council; it is not a statement of legal advice or screening. |
| Attribution required? | OFAC: no. UN: credit the UN Security Council Consolidated List. |
| Rate limits | None stated. Fetch each list once per run and do not poll faster than the lists change (OFAC and the UN update on business days). |

This is not a sanctions-screening tool: name matching, aliases and ownership
rules (such as the 50 Percent Rule) are out of scope. Use the lists' own
screening services for compliance decisions.

## Verification

DECIDED: stream the lists and keep nothing -- the sources are bulk files with no
query API, so a streamed filter goes ahead of any cache.

Offline tests use fixtures that keep the upstream layout with synthesized
values. The live test (`cargo test -p statsapi-sanctions -- --ignored`) streams
both lists for Iran.
