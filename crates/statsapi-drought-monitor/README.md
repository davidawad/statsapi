# statsapi-drought-monitor

`statsapi-drought-monitor` is a blocking client for the
[US Drought Monitor data services](https://droughtmonitor.unl.edu/DmData/DataDownload/WebServiceInfo.aspx).
It queries the keyless JSON API live (`CountyStatistics` by 5-digit county FIPS,
`StateStatistics` by 2-digit state FIPS) in the cumulative `statisticsType=1`
format: `d0` is the percent of area in D0-D4, `d1` in D1-D4, `d2` in D2-D4,
`d3` in D3-D4, `d4` in D4. It never caches or writes to disk.

## Usage

```rust
use statsapi_drought_monitor::{summarize, Area, Client, Date};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let end = Date::today();
    let weeks = Client::new()?.weekly(Area::County, "01001", end.plus_days(-400), end)?;
    println!("{:?}", summarize(&weeks));
    Ok(())
}
```

`summarize` returns the latest week, the change in D2-D4 percent versus 4 and
52 weeks earlier (`None` when that exact week is absent), and the count of weeks
with any D2-D4 area in the 52 weeks ending at the latest week. Requests retry
transient transport errors and HTTP 429/5xx up to three attempts.

## Terms

| Source | Automated access | Redistribution and attribution | Rate limits |
|---|---|---|---|
| [USDM data services](https://droughtmonitor.unl.edu/DmData/DataDownload/WebServiceInfo.aspx) | Public keyless web service intended for programmatic use. | Free to use; credit "The U.S. Drought Monitor is jointly produced by the National Drought Mitigation Center at the University of Nebraska-Lincoln, the United States Department of Agriculture, and the National Oceanic and Atmospheric Administration." | None numerically stated; keep requests small. |
